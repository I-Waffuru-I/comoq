use moq_lite::{BroadcastProducer, TrackProducer};

use crate::connected::ConnectedClient;


pub struct ShareFile {
    pub sync : TrackProducer,
    pub broadcast : BroadcastProducer,
    pub clients : Vec<ConnectedClient>,
}
impl ShareFile {
    pub fn add_client(&mut self, client : ConnectedClient){
        self.clients.push(client);
    }
}
