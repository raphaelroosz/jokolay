use crate::attributes::CommonAttributes;
use glam::Vec3;
use uuid::Uuid;


#[derive(Debug, Clone)]
pub struct Marker {
    pub guid: Uuid,
    pub parent: Uuid,
    pub position: Vec3,
    pub map_id: u32,
    pub category: String,
    pub source_file_name: String,
    pub attrs: CommonAttributes,
}
