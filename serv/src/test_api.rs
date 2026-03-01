use moq_native::{Server, ServerConfig};
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let origin = moq_native::moq_lite::Origin::produce();
    let mut server = Server::new(ServerConfig::default())?
        .with_publish(origin.producer.consume())
        .with_consume(origin.producer.clone());
    if let Some(request) = server.accept().await {
        let session = request.accept().await?;
        let _ = session.closed().await;
    }
    Ok(())
}
