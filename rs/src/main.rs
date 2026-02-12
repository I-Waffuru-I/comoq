use moq_lite::{Broadcast, BroadcastProducer, OriginConsumer, OriginProducer, Produce, TrackProducer};
use moq_native::{ClientConfig, ServerConfig};
use url::Url;
use clap::Parser;
use quinn::crypto::rustls::QuicServerConfig;
use tokio::{task::JoinSet, sync::RwLock};

use std::sync::Arc;

use crate::{connected::ConnectedClient, share::ShareFile};

mod connected;
mod share;


#[derive(Parser)]
struct AppConfig {
    #[arg(long)]
    pub url : Url,
}


#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::DEBUG).init();

    let config = AppConfig::parse();
    let mut origin = moq_lite::Origin::produce();
    let set = init_shared_files(&mut origin);
    
    // Not sure which to use. client gives runtime errors for the Quic connection
    // let client = moq_native::Client::new(ClientConfig::default())?;
    // let session = client.with_publish(origin.producer.consume()).connect(config.url).await?;

    let server_config = ServerConfig::default();
    let server = moq_native::Server::new(server_config)?;
    let session = server.with_publish(origin.producer.consume());

    let _ = set.join_all().await;

    Ok(())
}

/// spawn a tokio task for each shared file
fn init_shared_files(origin : &mut Produce<OriginProducer, OriginConsumer>)-> JoinSet<()> {
    let broadcast_names = vec!("comoq");
    let mut set : JoinSet<()> = JoinSet::new(); 
    for name in broadcast_names {
        let mut bc = Broadcast::produce();
        let track = bc.producer.create_track(moq_lite::Track {
            name : name.to_string(),
            priority: 0
        });
        let sf = ShareFile {
            sync : track,
            broadcast : bc.producer,
            clients : vec!(),
        };
        origin.producer.publish_broadcast(name, bc.consumer);
        println!("INIT: started broadcast [{name}]");
        set.spawn(async move {
            manage_sf(sf).await;
        });
    }
    set
}


async fn manage_sf(sf : ShareFile){
    let asf = Arc::new(RwLock::new(sf));

    println!("NEW_CONN: Start manage");
    let asf_conn = asf.clone();
    let new_connections_handle = tokio::spawn(async move {
        // clone the broadcast producer to avoid holding the lock across await
        let mut broadcast = {
            let conn = asf_conn.read().await;
            conn.broadcast.clone()
        };

        println!("NEW_CONN: listen incoming");
        while let Some(track) = broadcast.requested_track().await {
            let name = track.info.name.to_string();
            println!("NEW_CONN: requested track [{}]", name);
            let client = ConnectedClient { track, name };
            let mut write_conn = asf_conn.write().await;
            write_conn.add_client(client);
        }
    });

    let asf_sync = asf.clone();
    let handle_sync = tokio::spawn(async move {
        let mut last_clients_len = 0;
        loop {
            let (master, new_clients) = {
                let sf = asf_sync.read().await;
                if sf.clients.len() > last_clients_len {
                    let new = sf.clients[last_clients_len..].to_vec();
                    last_clients_len = sf.clients.len();
                    (Some(sf.sync.clone()), new)
                } else {
                    (None, vec![])
                }
            };

            if let Some(master) = master {
                for client in new_clients {
                    tokio::spawn(sync_file(client.track, master.clone()));
                }
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    let _ = tokio::join!(handle_sync, new_connections_handle);
}

async fn sync_file(client_track : TrackProducer, mut master_track : TrackProducer) {
    println!("start sync file");
    if let Ok(Some(mut group)) = client_track.consume().next_group().await {
        if let Ok(Some(frame )) = group.read_frame().await{
            println!("got bytes in track [{}]: ", client_track.info.name);
            dbg!(&frame);
            let mut gp = master_track.append_group();
            gp.write_frame(frame);
        }
    }

}
