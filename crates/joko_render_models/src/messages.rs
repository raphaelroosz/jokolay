mutually_exclusive_features::exactly_one_of!(
    "messages_any",
    "messages_bincode",
    "messages_downcast"
);

use joko_component_models::ComponentDataExchange;
use serde::{Deserialize, Serialize};

use crate::{marker::MarkerObject, trail::TrailObject};

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageToRenderer {
    BulkMarkerObject(Vec<MarkerObject>),
    BulkTrailObject(Vec<TrailObject>),
    //Present,// a render loop is finished and we can present it
    MarkerObject(Box<MarkerObject>),
    RenderSwapChain, // The list of elements to display was changed
    TrailObject(Box<TrailObject>),
}

/*
impl From<MessageToRenderer> for ComponentDataExchange {
    fn from(src: MessageToRenderer) -> ComponentDataExchange {
        bincode::serialize(&src).unwrap() //shall crash if wrong serialization of messages
    }
}

#[allow(clippy::from_over_into)]
impl Into<MessageToRenderer> for ComponentDataExchange {
    fn into(self) -> MessageToRenderer {
        bincode::deserialize(&self).unwrap()
    }
}
*/
