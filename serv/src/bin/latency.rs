use std::{
    collections::HashMap, fmt::Display, sync::{Arc, RwLock}, time::{Duration, Instant}
};

use chrono::Local;
use moq_lite::{Broadcast, Track};
use moq_native::ClientConfig;
use operational_transform::OperationSeq;
use tokio::sync::Mutex;
use url::Url;


struct Logger <T> {
    pub snt : Vec<T>,
    pub rcv : Vec<T>,
}
impl<T : Display+Clone> Logger<T>  {

    pub fn new() -> Logger<T> {
        Logger{
            snt : vec!(),
            rcv : vec!(),

        }
    }
    pub fn save(&self, filename: &str){
        let mut output = String::new();
        let mut items : Vec<(T, T)> = vec!();

        for i in 0..self.snt.len() {
            if let Some(v1) = self.snt.iter().nth(i)
              && let Some(v2) = self.rcv.iter().nth(i) {
                  items.insert(i, (v1.clone(), v2.to_owned()))
            }

        }

        for (f,s) in items.iter() {
            output.push_str(&format!("{};{}\r\n", f,s));
        }

        _ = std::fs::write(filename, output);
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::INFO).init();

    let url = Url::parse("https://127.0.0.1:4443")?;

    let mut config = ClientConfig::default();
    config.tls.disable_verify = Some(true);

    let bc_name = "file1";
    let client_id = "latency_probe";
    let own_bc_name = format!("{bc_name}/client/{client_id}");
    
    let logger = Arc::new(RwLock::new(Logger::<chrono::DateTime<Local>>::new()));

    // -- origins ---------------------------------------------------------------
    let read_origin = moq_lite::Origin::produce();
    let publish_origin = moq_lite::Origin::produce();

    // -- our broadcast (update track) -----------------------------------------
    let mut client_bc = Broadcast::produce();
    let mut update_track = client_bc.producer.create_track(Track {
        name: "update".into(),
        priority: 1,
    });
    // Publish under the name the server expects: `file1/client/latency_probe`
    publish_origin
        .producer
        .publish_broadcast(&own_bc_name, client_bc.consumer);

    // -- connect ---------------------------------------------------------------
    let _session = moq_native::Client::new(config)?
        .with_consume(read_origin.producer.clone())
        .with_publish(publish_origin.consumer.clone())
        .connect(url)
        .await?;

    println!("Connected. Waiting for server broadcast '{bc_name}'…");

    let pending: Arc<Mutex<HashMap<u64, Instant>>> = Arc::new(Mutex::new(HashMap::new()));

    let pending_recv = pending.clone();
    let own_bc_filter = own_bc_name.clone();
    let mut origin_consumer = read_origin.consumer;
    let sub_logger = logger.clone();

    let subscribe_task = tokio::spawn(async move {
        while let Some((path, broadcast)) = origin_consumer.announced().await {
            let path_str = path.to_string();
            // Skip our own broadcast echoed back
            if path_str == own_bc_filter {
                continue;
            }
            println!("Broadcast announced: [{path_str}]");

            let Some(broadcast) = broadcast else {
                continue;
            };

            if path_str != bc_name {
                println!("  (ignoring, not '{bc_name}')");
                continue;
            }

            let sub_sub_logger = sub_logger.clone();
            let pending_inner = pending_recv.clone();
            let client_id_owned = client_id.to_string();
            tokio::spawn(async move {
                let mut sync_consumer = broadcast.subscribe_track(&moq_lite::Track::new("sync"));
                println!(
                    "Subscribed to [{path_str}/sync] (track: {})",
                    sync_consumer.info.name
                );

                while let Ok(Some(mut group)) = sync_consumer.next_group().await {
                    while let Ok(Some(frame)) = group.read_frame().await {
                        let Ok(text) = String::from_utf8(frame.to_vec()) else {
                            continue;
                        };
                        // Packet format from server: "client_id;version;json_op"
                        let parts: Vec<&str> = text.splitn(3, ';').collect();
                        if parts.len() < 2 {
                            continue;
                        }
                        let from_client = parts[0];
                        let version: u64 = match parts[1].parse() {
                            Ok(v) => v,
                            Err(_) => continue,
                        };

                        println!("RECEIVE {} {:#?}", version, std::time::Instant::now());
                        if let Ok(mut lg) = sub_sub_logger.write() {
                            lg.rcv.push(chrono::Local::now());
                        }

                        // Only measure our own echoes
                        if from_client != client_id_owned {
                            println!("  sync from other client '{from_client}' v{version}");
                            continue;
                        }

                        let mut map = pending_inner.lock().await;
                        if let Some(sent_at) = map.remove(&version) {
                            let rtt = sent_at.elapsed();
                            println!("RTT v{version}: {rtt:?}");
                        } else {
                            println!("  sync echo v{version} (no pending probe)");
                        }
                    }
                }
            });
        }
    });

    // -- periodically send OT changes -----------------------------------------
    // We need to track the current document length so our OT ops are valid.
    // The server starts with `"moq echo for file1"` (18 chars).
    let doc_len = Arc::new(Mutex::new(18u64)); // length of "moq echo for file1"
    let pending_send = pending.clone();
    let pub_logger = logger.clone();

    let publish_task = tokio::spawn(async move {
        // Give the connection a moment to establish subscriptions
        tokio::time::sleep(Duration::from_secs(2)).await;

        let mut counter: u64 = 0;
        loop {
            counter += 1;

            let probe_text = format!("p{counter}");
            let probe_len = probe_text.len() as u64;

            // Build a valid OT op: retain(current_len) + insert(probe_text)
            let current_len = {
                let l = doc_len.lock().await;
                *l
            };
            let mut op = OperationSeq::default();
            op.retain(current_len);
            op.insert(&probe_text);

            let json = serde_json::to_string(&op)?;
            {
                let mut map = pending_send.lock().await;
                map.insert(counter, Instant::now());
            }

            println!("SENDING {counter} {:#?}", std::time::Instant::now());
            if let Ok(mut lg) = pub_logger.write() {
                lg.snt.push(chrono::Local::now());
            }
            update_track.write_frame(bytes::Bytes::from(json));

            // Update our local doc length estimate
            {
                let mut l = doc_len.lock().await;
                *l += probe_len;
            }

            tokio::time::sleep(Duration::from_secs(3)).await;
        }

        #[allow(unreachable_code)]
        Ok::<(), anyhow::Error>(())
    });

    tokio::select! {
        _ = subscribe_task => {},
        _ = publish_task => {},
        _ = tokio::signal::ctrl_c() => {
            println!("\nShutting down.");
        },
    }
    match logger.read() {
        Ok(lg)=>{
            println!("saving data");
            lg.save("latency_log.csv");
        }
        Err(_)=>{}
    }

    Ok(())
}
