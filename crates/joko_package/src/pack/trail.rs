use uuid::Uuid;

use super::CommonAttributes;

#[derive(Debug, Clone)]
pub(crate) struct Trail {
    pub guid: Uuid,
    pub parent: Uuid,
    pub map_id: u32,
    pub category: String,
    pub props: CommonAttributes,
    pub dynamic: bool,
    pub source_file_name: String,
}

#[derive(Debug, Clone)]
pub(crate) struct TBin {
    pub map_id: u32,
    pub version: u32,
    pub nodes: Vec<glam::Vec3>,
}
#[derive(Debug, Clone)]
pub(crate) struct TBinStatus {
    pub tbin: TBin,
    pub iso_x: bool,
    pub iso_y: bool,
    pub iso_z: bool,
    pub closed: bool,
}

impl TBin {}
