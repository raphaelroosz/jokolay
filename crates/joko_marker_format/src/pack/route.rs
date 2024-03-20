use uuid::Uuid;
use glam::Vec3;

#[derive(Debug, Clone)]
pub(crate) struct Route {
    pub category: String,
    pub path: Vec<Vec3>,
    pub reset_position: Vec3,
    pub reset_range: f64,
    pub map_id: u32,
    pub guid: Uuid,
    pub name: String,
    pub source_file_name: String,
}
