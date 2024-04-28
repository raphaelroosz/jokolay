mutually_exclusive_features::exactly_one_of!("messages_any", "messages_bincode");

use joko_component_models::{to_data, ComponentDataExchange};
use serde::{Deserialize, Serialize};

use crate::MumbleLink;

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageToMumbleLinkBack {
    BindedOnUI,
    Autonomous,
    Value(Option<MumbleLink>), //pushed from a value imposed by UI. Either a form or a traveling for demo.
}

/*
impl From<MessageToMumbleLinkBack> for ComponentDataExchange {
    fn from(src: MessageToMumbleLinkBack) -> ComponentDataExchange {
        to_data(src)
    }
}

#[allow(clippy::from_over_into)]
impl Into<MessageToMumbleLinkBack> for ComponentDataExchange {
    fn into(self) -> MessageToMumbleLinkBack {
        bincode::deserialize(&self).unwrap()
    }
}
*/
