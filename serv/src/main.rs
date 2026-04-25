use axum::{Router, routing::get};
use clap::Parser;
use moq_lite::{Broadcast, Origin, OriginConsumer, OriginProducer, Track};
use moq_native::ServerConfig;
use operational_transform::OperationSeq;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tokio::{
    sync::{RwLock, broadcast::channel},
    task::JoinSet,
};
use tower_http::cors::{Any, CorsLayer};

#[derive(clap::Parser)]
struct ArgList {
    #[arg(short, long)]
    pub url: String,
    #[arg(short, long)]
    pub cert_path: String,
    #[arg(short, long)]
    pub key_path: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::INFO).init();

    let arglist = ArgList::parse();

    let mut config = ServerConfig::default();
    let files = vec!["file1"];

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
    let mut joinset: JoinSet<()> = JoinSet::new();
    let _ = setup_cors_stuff(fingerprints.get(0).unwrap().clone(), &mut joinset).await;

    println!("Server started, listening for tracks in broadcast 'echo'...");

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
        joinset.spawn(run_file(file, pub_clone.producer, con_clone.consumer));
    }

    joinset.join_all().await;

    Ok(())
}

async fn run_file(
    file_name: &str,
    publish_origin: OriginProducer,
    mut consume_origin: OriginConsumer,
) {
    let mut bc = Broadcast::produce();
    publish_origin.publish_broadcast(file_name, bc.consumer);

    let initial_text = format!("moq echo for {file_name}");
    let mut initial_op = OperationSeq::default();
    initial_op.insert(&initial_text);
    let doc_state = Arc::new(RwLock::new(initial_op));
    let doc_version = Arc::new(RwLock::new(0u64));

    // sends the OT updates to clients
    let mut sync_track = bc.producer.create_track(moq_lite::Track {
        name: "sync".to_string(),
        priority: 0,
    });
    let (tx_changes, mut rcv_changes) = channel::<(String, u64, OperationSeq)>(50);
    let send_changes_thread = tokio::spawn(async move {
        loop {
            match rcv_changes.recv().await {
                Ok((client, version, op)) => {
                    let json_op = serde_json::to_string(&op).unwrap();
                    let packet = format!("{};{};{}", client, version, json_op);
                    // println!("packet: {packet}");
                    sync_track.write_frame(bytes::Bytes::from(packet));
                }
                Err(_) => {}
            }
        }
    });

    // keeps track of file state
    let mut state_track = bc.producer.create_track(moq_lite::Track {
        name: "state".to_string(),
        priority: 0,
    });
    let (tx_state, mut rcv_state) = channel::<bool>(50);
    let tx_state_clone = tx_state.clone();
    let doc_clone_state = doc_state.clone();
    let version_state = doc_version.clone();
    let send_state_thread = tokio::spawn(async move {
        let mut doc_last_save = doc_clone_state
            .read()
            .await
            .apply("")
            .unwrap_or(String::new());
        // println!("state save : [{}]", doc_last_save);
        loop {
            match rcv_state.recv().await {
                Ok(d) => {
                    let doc = doc_clone_state.read().await;
                    if let Ok(txt) = doc.apply("") {
                        let version = *version_state.read().await;
                        if txt != doc_last_save || d {
                            // println!("state (v{}): [{}]", version, txt);
                            let packet = format!("{};{}", version, txt);
                            state_track.write_frame(bytes::Bytes::from(packet));
                            doc_last_save = txt;
                        }
                    }
                }
                Err(_) => {}
            }
        }
    });
    let manage_state_thread = tokio::spawn(async move {
        loop {
            let _ = tokio::time::sleep(Duration::from_secs(1)).await;
            let _ = tx_state_clone.send(false);
        }
    });

    // handle connecting clients and merge their changes
    let tx_state_clone = tx_state.clone();
    let doc_receive_clone = doc_state.clone();
    let name_clone = file_name.to_string();
    let receive_thread = tokio::spawn(async move {
        println!("START LISTEN FOR UPDATE BCs IN {name_clone}");
        while let Some(announced) = consume_origin.announced().await {
            println!("\nUPDATE BC ANNOUNCED [{}]\n", announced.0);
            // ignore bc that aren't directed at this file
            // maybe it messes with other threads?
            if !announced.0.to_string().starts_with(&format!("{name_clone}/client/")) {
                println!("ANNOUNCE SKIP [{}] by [{}]", announced.0, name_clone);
                continue;
            }
            let bc = if let Some(b) = announced.1 {
                b
            } else {
                println!("CLOSED");
                continue;
            };

            // send update to state
            let _ = tx_state_clone.send(true);

            // find update track of client to read changes from
            let doc_inner_clone = doc_receive_clone.clone();
            let doc_version_clone = doc_version.clone();
            let tx_inner = tx_changes.clone();
            let _handle_bc = tokio::spawn(async move {
                let client_id = announced.0.to_string().split("/").nth(2)
                    .unwrap_or(&announced.0.to_string()).to_string();
                println!("start thread for updates track [{}]", client_id);
                let track_name = String::from("update");
                let track = Track {
                    name: track_name.clone(),
                    priority: 0,
                };
                let mut client_track = bc.subscribe_track(&track);
                while let Ok(Some(mut group)) = client_track.next_group().await {
                    while let Ok(Some(frame)) = group.read_frame().await {
                        let c = client_id.clone();
                        let mut doc = doc_inner_clone.write().await;
                        let mut version = doc_version_clone.write().await;
                        if let Ok(incoming_op) = serde_json::from_slice::<OperationSeq>(&frame) {
                            // neem aan dat incoming_op.len() == doc.len()
                            match doc.compose(&incoming_op) {
                                Ok(new_doc) => {
                                    *doc = new_doc;
                                    *version += 1;
                                    // println!("[{}] updated (v{}): [{}]", c, *version, doc.apply("").unwrap());
                                    let _ = tx_inner.send((c, *version, incoming_op));
                                }
                                Err(e) => {
                                    eprintln!("[{}] Failed to merge OT op: {}", c, e);
                                }
                            }
                        } else if let Ok(text) = String::from_utf8(frame.to_vec()) {
                            // Fallback for simple text updates if they don't look like JSON array of ops
                            *version += 1;
                            // println!("[{}] updated (v{}) (fallback) : [{}]", c, *version, text);
                            let mut new_op = OperationSeq::default();
                            new_op.insert(&text);
                            *doc = new_op.clone();
                            let _ = tx_inner.send((c, *version, new_op));
                        }
                    }
                }
                println!("\tdone handle bc [{}]", announced.0.clone());
            });
            // let _ = handle_bc.await;
        }
    });
    let _ = tokio::join!(
        send_changes_thread,
        send_state_thread,
        manage_state_thread,
        receive_thread
    );
}

async fn setup_cors_stuff(fingerprint: String, join_set: &mut JoinSet<()>) -> anyhow::Result<()> {
    // simpele http server om ne get van sha fingerprints te voorzien
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/certificate.sha256", get(fingerprint))
        .layer(cors);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:4443").await?;
    join_set.spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    Ok(())
}

// generated these, cba to write tests but they're a nice example to figure out how this OT works
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding() {
        let mut op = OperationSeq::default();
        op.retain(3);
        op.insert("hello");
        op.delete(2);

        let serialized = serde_json::to_string(&op).unwrap();
        // [3, "hello", -2]
        assert!(serialized.contains("3"));
        assert!(serialized.contains("\"hello\""));
        assert!(serialized.contains("-2"));

        let deserialized: OperationSeq = serde_json::from_str(&serialized).unwrap();
        assert_eq!(op, deserialized);
    }

    #[test]
    fn test_merge_compose() {
        let mut doc = OperationSeq::default();
        doc.insert("hello");

        let mut edit = OperationSeq::default();
        edit.retain(5);
        edit.insert(" world");

        let merged = doc.compose(&edit).unwrap();
        assert_eq!(merged.apply("").unwrap(), "hello world");
    }

    #[test]
    fn test_apply_error() {
        let mut doc = OperationSeq::default();
        doc.insert("abc");

        let mut edit = OperationSeq::default();
        edit.retain(5);

        let result = doc.compose(&edit);
        assert!(result.is_err());
    }
}
