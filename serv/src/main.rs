use moq_lite::{Broadcast, Origin, Track};
use moq_native::ServerConfig;
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::DEBUG).init();

    let mut config = ServerConfig::default();
    config.bind = Some("127.0.0.1:4443".parse()?);
    config.tls.generate = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    // publish to clients (echo)
    let publish_origin = Origin::produce();
    // consume from clients
    let mut consume_origin = Origin::produce();
    let mut server = moq_native::Server::new(config)?
        .with_publish(publish_origin.consumer.clone())
        .with_consume(consume_origin.producer.clone());

    let mut bc = Broadcast::produce();
    // main track to push
    let main_t = bc.producer.create_track(moq_lite::Track {
        name: "main".to_string(),
        priority: 0,
    });
    let pushed_string = Arc::new(RwLock::new(String::from("moq echo text")));

    let main_track = Arc::new(RwLock::new(main_t));

    publish_origin
        .producer
        .publish_broadcast("echo", bc.consumer);

    println!("Server started, listening for tracks in broadcast 'echo'...");

    // task to accept incoming connections
    let accepting_thread = tokio::spawn(async move {
        println!("START ACCEPTING");
        while let Some(request) = server.accept().await {
            println!("ACCEPT CONNECTION");
            tokio::spawn(async move {
                match request.accept().await {
                    Ok(session) => {
                        let _ = session.closed().await;
                    }
                    Err(err) => {
                        tracing::warn!(%err, "failed to accept MoQ session");
                    }
                }
            });
        }
    });

    // sends the full string once per few seconds
    let text_clone = pushed_string.clone();
    let main_track_clone = main_track.clone();
    let send_thread = tokio::spawn(async move {
        loop {
            let text = text_clone.read().await.clone();
            let mut main = main_track_clone.write().await;
            main.write_frame(bytes::Bytes::from(text));
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });

    let push_str_clone = pushed_string.clone();
    let receive_thread = tokio::spawn(async move {
        println!("START LISTEN FOR UPDATE TRACKS");
        while let Some(announced) = consume_origin.consumer.announced().await {
            println!("\nANNOUNCED [{}]\n", announced.0);
            let bc = announced.1.expect("no broadcast in announced..?");

            let push_str_inner_clone = push_str_clone.clone();
            let handle_bc = tokio::spawn(async move {
                let track_name = String::from("client_1");
                let track = Track {
                    name: track_name.clone(),
                    priority: 0,
                };
                let mut client_track = bc.subscribe_track(&track);
                println!("\tstart handle bc [{}]", announced.0.clone());
                while let Ok(Some(mut group)) = client_track.next_group().await {
                    while let Ok(Some(frame)) = group.read_frame().await {
                        if let Ok(text) = String::from_utf8(frame.to_vec()) {
                            println!("Update: [{}]", text);
                            let mut ps = push_str_inner_clone.write().await;
                            *ps = text;
                        }
                    }
                }
                println!("\tdone handle bc [{}]", announced.0.clone());
            });
            let _ = handle_bc.await;
        }
    });
    let _ = tokio::join!(accepting_thread, send_thread, receive_thread);
    Ok(())
}
