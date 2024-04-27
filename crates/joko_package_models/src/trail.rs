use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::attributes::CommonAttributes;
use joko_core::serde_glam::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trail {
    pub guid: Uuid,
    pub parent: Uuid,
    pub map_id: u32,
    pub category: String,
    pub props: CommonAttributes,
    pub dynamic: bool,
    pub source_file_uuid: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TBin {
    pub map_id: u32,
    pub version: u32,
    pub nodes: Vec<Vec3>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TBinStatus {
    pub tbin: TBin,
    pub iso_x: bool,
    pub iso_y: bool,
    pub iso_z: bool,
    pub closed: bool,
}

impl TBin {}
