use crate::attributes::CommonAttributes;
use joko_core::serde_glam::Vec3;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    pub guid: Uuid,
    pub parent: Uuid,
    pub position: Vec3,
    pub map_id: u32,
    pub category: String,
    pub source_file_uuid: Uuid,
    pub attrs: CommonAttributes,
}
