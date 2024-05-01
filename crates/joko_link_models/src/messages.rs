use serde::{Deserialize, Serialize};

use crate::MumbleLink;

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageToMumbleLinkBack {
    BindedOnUI,
    Autonomous,
    Value(Option<MumbleLink>), //pushed from a value imposed by UI. Either a form or a traveling for demo.
}
