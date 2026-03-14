use clap::Parser;
use moq_lite::{Broadcast, Origin, OriginConsumer, OriginProducer, Track};
use moq_native::ServerConfig;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{sync::RwLock, task::JoinSet};
use axum::{Router, routing::get};
use tower_http::cors::{Any, CorsLayer};


#[derive(clap::Parser)]
struct ArgList {
    #[arg(short, long)]
    pub url : String,
    #[arg(short, long)]
    pub cert_path : String,
    #[arg(short, long)]
    pub key_path : String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::DEBUG).init();

    let arglist = ArgList::parse();

    let mut config = ServerConfig::default();
    let files = vec!("file1");

    config.bind = Some(arglist.url.parse()?);
    config.tls.cert.push(PathBuf::from(arglist.cert_path));
    config.tls.key.push(PathBuf::from(arglist.key_path));

    // publish to clients 
    let publish_origin = Origin::produce();
    // consume from clients
    let consume_origin = Origin::produce();
    let mut server = moq_native::Server::new(config)?
        .with_publish(publish_origin.consumer.clone())
        .with_consume(consume_origin.producer.clone());

    let fingerprints = server.tls_info().read().unwrap().fingerprints.clone();
    println!("FINGERPRINTS");
    dbg!(&fingerprints);
    let _ = setup_cors_stuff(fingerprints.get(0).unwrap().clone()).await;


    println!("Server started, listening for tracks in broadcast 'echo'...");

    let mut joinset : JoinSet<()> = JoinSet::new();

    // task to accept incoming connections
    joinset.spawn(async move {
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

    for file in files {
        let pub_clone = publish_origin.clone();
        let con_clone = consume_origin.clone();
        joinset.spawn(
            run_file(file, pub_clone.producer, con_clone.consumer)
        );
    }
    joinset.join_all().await;

    Ok(())
}

async fn run_file(file_name: &str, publish_origin : OriginProducer, mut consume_origin : OriginConsumer){
    let mut bc = Broadcast::produce();
    // main track to push
    let main_t = bc.producer.create_track(moq_lite::Track {
        name: "sync".to_string(),
        priority: 0,
    });
    let pushed_string = Arc::new(RwLock::new(format!("moq echo for {file_name}")));
    let sync_track = Arc::new(RwLock::new(main_t));

    publish_origin.publish_broadcast(file_name, bc.consumer);

    // sends the full string once per few seconds
    let text_clone = pushed_string.clone();
    let sync_track_clone = sync_track.clone();
    let name_clone = file_name.to_string();
    let send_thread = tokio::spawn(async move {
        loop {
            let text = text_clone.read().await.clone();
            let mut track = sync_track_clone.write().await;
            //println!("sync bc [{name_clone}] : [{text}]");
            track.write_frame(bytes::Bytes::from(text));
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });

    // handle connecting clients and merge their changes
    let push_str_clone = pushed_string.clone();
    let name_clone = file_name.to_string();
    let receive_thread = tokio::spawn(async move {
        println!("START LISTEN FOR UPDATE BCs IN {name_clone}");
        while let Some(announced) = consume_origin.announced().await {
            println!("\nANNOUNCED [{}]\n", announced.0);
            // ignore bc that aren't directed at this file
            // maybe it messes with other threads?
            if !announced.0.to_string().starts_with(&format!("{name_clone}/client/")) {
                println!("ANNOUNCE SKIP [{}] by [{}]", announced.0, name_clone);
                continue
            }
            if announced.1.is_none() {
                println!("CLOSED");
                continue
            }
            let bc = announced.1.unwrap();

            // find update track of client to read changes from
            let push_str_inner_clone = push_str_clone.clone();
            let handle_bc = tokio::spawn(async move {
                let track_name = String::from("update");
                let track = Track {
                    name: track_name.clone(),
                    priority: 0,
                };
                let mut client_track = bc.subscribe_track(&track);
                while let Ok(Some(mut group)) = client_track.next_group().await {
                    while let Ok(Some(frame)) = group.read_frame().await {
                        if let Ok(text) = String::from_utf8(frame.to_vec()) {
                            println!("Update text over bc [{}]: [{}]", announced.0, text);
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
    let _ = tokio::join!(send_thread, receive_thread);
}

async fn setup_cors_stuff(fingerprint : String) -> anyhow::Result<()>{

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/certificate.sha256", get(fingerprint))
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:4443").await?;
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Ok(())
}
