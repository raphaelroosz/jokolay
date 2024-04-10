use joko_core::RelativePath;
use uuid::Uuid;
use glam::Vec3;

use crate::pack::CommonAttributes;

use super::{TBin, Trail};

#[derive(Debug, Clone)]
pub(crate) struct Route {
    pub category: String,
    pub parent: Uuid,
    pub path: Vec<Vec3>,
    pub reset_position: Vec3,
    pub reset_range: f64,
    pub map_id: u32,
    pub guid: Uuid,
    pub name: String,
    pub source_file_name: String,
}
