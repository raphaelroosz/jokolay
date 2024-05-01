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
