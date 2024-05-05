use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageToApplicationBack {
    SaveUIConfiguration(String),
}
