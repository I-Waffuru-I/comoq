use moq_lite::{Broadcast, Origin};
use moq_native::ServerConfig;
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::DEBUG).init();

    let mut config = ServerConfig::default();
    config.bind = Some("127.0.0.1:4443".parse()?);
    config.tls.generate = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let origin = Origin::produce();
    //let producer = origin.producer.consume();
    let mut server = moq_native::Server::new(config)?
        .with_publish(origin.consumer.clone());

    let mut bc = Broadcast::produce();
    // main track to push
    let main_t = bc.producer.create_track(moq_lite::Track {
        name: "main".to_string(),
        priority: 0,
    });
    let pushed_string = Arc::new(RwLock::new(String::from("moq echo text")));

    let main_track = Arc::new(RwLock::new(main_t));
    let mut bc_producer = bc.producer.clone();

    origin.producer.publish_broadcast("echo", bc.consumer);

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

    // sends the full string once per second
    let text_clone = pushed_string.clone();
    let main_track_clone = main_track.clone();
    let send_thread = tokio::spawn(async move {
        loop {
            let text = text_clone.read().await.clone();
            let mut main = main_track_clone.write().await;
            println!("ECHO: [{}]", text);
            main.write_frame(bytes::Bytes::from(text));

            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });

    let receive_thread = tokio::spawn(async move {

        println!("START LISTEN FOR UPDATE TRACKS");
        while let Some(client_track) = bc_producer.requested_track().await {
            let name = client_track.info.name.clone();
            println!("Handle new track: [{}]", name);
            if name == "main" {
                continue;
            }
            let text_clone = pushed_string.clone();
            tokio::spawn(async move {
                let mut consumer = client_track.consume();
                while let Ok(Some(mut group)) = consumer.next_group().await {
                    while let Ok(Some(frame)) = group.read_frame().await {
                        let frame: bytes::Bytes = frame;
                        if let Ok(text) = String::from_utf8(frame.to_vec()) {
                            println!("UPDATE: [{}]: [{}]", name, text);
                            let mut push_text = text_clone.write().await;
                            *push_text = text;
                        }
                    }
                }
                println!("Client update track closed: {}", name);
            });
        }
    });
    let _ = tokio::join!(accepting_thread,send_thread,receive_thread);
    Ok(())
}
