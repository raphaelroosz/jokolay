use joko_components::ComponentDataExchange;
use serde::{Deserialize, Serialize};

use crate::{marker::MarkerObject, trail::TrailObject};

#[derive(Serialize, Deserialize)]
pub enum UIToUIMessage {
    BulkMarkerObject(Vec<MarkerObject>),
    BulkTrailObject(Vec<TrailObject>),
    //Present,// a render loop is finished and we can present it
    MarkerObject(Box<MarkerObject>),
    RenderSwapChain, // The list of elements to display was changed
    TrailObject(Box<TrailObject>),
}

impl From<UIToUIMessage> for ComponentDataExchange {
    fn from(src: UIToUIMessage) -> ComponentDataExchange {
        bincode::serialize(&src).unwrap() //shall crash if wrong serialization of messages
    }
}
