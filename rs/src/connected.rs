use moq_lite::TrackProducer;


#[derive(Clone)]
pub struct ConnectedClient{
    pub track : TrackProducer,
    pub name : String,

}
impl ConnectedClient {
    pub async fn run(&self) -> anyhow::Result<()>{


        Ok(())
    }
}
