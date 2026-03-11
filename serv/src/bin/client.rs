
use moq_lite::{Broadcast, BroadcastConsumer, OriginProducer};
use moq_native::ClientConfig;
use tokio::task::JoinSet;
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::DEBUG).init();

    let url = Url::parse("https://127.0.0.1:4443")?;

    let mut config = ClientConfig::default();
    config.tls.disable_verify = Some(true);

    let mut origin = moq_lite::Origin::produce();

    let _session = moq_native::Client::new(config)?
        // session writes incoming broadcasts here
        .with_consume(origin.producer.clone())  
        // session reads from here to publish upstream
        .with_publish(origin.consumer.clone()) 
        .connect(url)
        .await?;

    println!("Waiting for broadcast 'echo'...");

    let mut thread_set : JoinSet<()> = JoinSet::new();
    while let Some((path, broadcast)) = origin.consumer.announced().await {
        println!("broadcast announced: [{}]", path);
        if let Some(broadcast) = broadcast {
            println!("Subscribed to broadcast [{}]", path);

            thread_set.spawn(run(broadcast, origin.producer.clone()));
        }
    }

    let _ = thread_set.join_all();

    Ok(())
}

async fn run(broadcast : BroadcastConsumer, origin_producer : OriginProducer){
    let mut track_consumer = broadcast.subscribe_track(&moq_lite::Track::new("main"));
    
    
    println!("Subscribed to track 'main'");

    let mut bc = Broadcast::produce();
    // main track to push
    let main_t = bc.producer.create_track(moq_lite::Track {
        name: "main".to_string(),
        priority: 0,
    });
    let mut counter : usize = 0;
    while let Ok(Some(mut group)) = track_consumer.next_group().await {
        if counter > 5 {
            return 
        }
        while let Ok(Some(frame)) = group.read_frame().await {
            dbg!(&frame);
            if let Ok(text) = String::from_utf8(frame.to_vec()) {
                counter += 1;
                println!("MAIN TRACK: {}", text);
            }
        }
    }
}
