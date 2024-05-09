use serde::{Deserialize, Serialize};

use crate::MumbleLink;

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageToMumbleLink {
    BindedOnUI,
    Autonomous,
    Value(MumbleLink), //pushed from a value imposed by UI. Either a form or a traveling for demo.
}
