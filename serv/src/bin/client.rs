use moq_native::ClientConfig;
use url::Url;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    moq_native::Log::new(tracing::Level::DEBUG).init();

    let url = Url::parse("https://127.0.0.1:4443")?;

    let mut config = ClientConfig::default();
    config.tls.disable_verify = Some(true);

    let origin = moq_lite::Origin::produce();
    let mut consumer = origin.consumer; // OriginConsumer — for reading announcements

    let _session = moq_native::Client::new(config)?
        // session writes incoming broadcasts here
        .with_consume(origin.producer)  
        // session reads from here to publish upstream
        //.with_publish(consumer.clone()) 
        .connect(url)
        .await?;

    println!("Waiting for broadcast 'echo'...");

    while let Some((path, broadcast)) = consumer.announced().await {
        println!("broadcast announced: [{}]", path);
        if let Some(broadcast) = broadcast {
            println!("Subscribed to broadcast [{}]", path);

            let mut track_consumer = broadcast.subscribe_track(&moq_lite::Track::new("main"));
            println!("Subscribed to track 'main'");

            while let Ok(Some(mut group)) = track_consumer.next_group().await {
                while let Ok(Some(frame)) = group.read_frame().await {
                    dbg!(&frame);
                    if let Ok(text) = String::from_utf8(frame.to_vec()) {
                        println!("MAIN TRACK: {}", text);
                    }
                }
            }
        }
    }

    Ok(())
}
