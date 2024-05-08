use serde::{Deserialize, Serialize};

use crate::{marker::MarkerObject, trail::TrailObject};

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageToRenderer {
    BulkMarkerObject(Vec<MarkerObject>),
    BulkTrailObject(Vec<TrailObject>),
    //Present,// a render loop is finished and we can present it
    MarkerObject(Box<MarkerObject>),
    RenderBegin,     // There is a change in what to display, reset current build
    RenderSwapChain, // The list of elements to display was changed. Or camera or position was changed.
    RenderFlush,     // Force whatever is being constructed to be kept and be what to display
    TrailObject(Box<TrailObject>),
}
