use std::time::Duration;

use moq_lite::{Broadcast, Track};
use moq_native::ClientConfig;
use tokio::task::JoinSet;
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::DEBUG).init();

    let url = Url::parse("https://127.0.0.1:4443")?;

    let mut config = ClientConfig::default();
    config.tls.disable_verify = Some(true);

    let bc_name = String::from("file1");
    let track_name = String::from("client_1");

    let read_origin = moq_lite::Origin::produce();
    let publish_origin = moq_lite::Origin::produce();
    let mut client_bc = Broadcast::produce();
    let mut client_track = client_bc.producer.create_track(Track {
        name: track_name.clone(),
        priority: 0,
    });
    publish_origin.producer
        .publish_broadcast(bc_name, client_bc.consumer);

    // Register the broadcast with the local origin so the session will announce and
    // send it to the server.

    let _session = moq_native::Client::new(config)?
        // The session will write broadcasts it receives from the server here.
        .with_consume(read_origin.producer.clone())
        // The session reads from here to publish upstream (sends our "client" broadcast).
        .with_publish(publish_origin.consumer.clone())
        .connect(url)
        .await?;

    println!("Connected. Publishing 'client' broadcast with 'client' track.");
    println!("Waiting for broadcast 'echo' from server...");

    // Spawn a task that periodically writes a short string into our published track.
    let publish_task = tokio::spawn(async move {
        let mut counter: u32 = 0;
        loop {
            let msg = format!("update from client #{counter}");
            println!("PUBLISH: {msg}");
            client_track.write_frame(bytes::Bytes::from(msg));
            counter += 1;
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
    });

    // Watch for broadcasts announced by the server and subscribe to the "main" track
    // of the "echo" broadcast.
    let mut origin_consumer = read_origin.consumer;
    let mut thread_set: JoinSet<()> = JoinSet::new();
    while let Some((path, broadcast)) = origin_consumer.announced().await {
        println!("broadcast announced: [{}]", path);
        if let Some(broadcast) = broadcast {
            let path_str = path.to_string();
            if path_str == track_name {
                continue
            }
            thread_set.spawn(async move {
                let mut track_consumer = broadcast.subscribe_track(&moq_lite::Track::new("main"));

                println!("Subscribed to track 'main' on broadcast '{path_str}'");

                let mut count = 0usize;
                while let Ok(Some(mut group)) = track_consumer.next_group().await {
                    if count > 10 {
                        break;
                    }
                    while let Ok(Some(frame)) = group.read_frame().await {
                        if let Ok(text) = String::from_utf8(frame.to_vec()) {
                            count += 1;
                            println!("[{path_str}/main] #{count}: {text}");
                        }
                    }
                }
            });
        }
    }

    let _ = thread_set.join_all().await;
    publish_task.abort();

    Ok(())
}
