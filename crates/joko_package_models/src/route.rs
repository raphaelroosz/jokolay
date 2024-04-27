use joko_core::{serde_glam::Vec3, RelativePath};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    attributes::CommonAttributes,
    trail::{TBin, Trail},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub category: String,
    pub parent: Uuid,
    pub path: Vec<Vec3>,
    pub reset_position: Vec3,
    pub reset_range: f64,
    pub map_id: u32,
    pub guid: Uuid,
    pub name: String,
    pub source_file_uuid: Uuid,
}

pub(crate) fn route_to_tbin(route: &Route) -> TBin {
    assert!(route.path.len() > 1);
    TBin {
        map_id: route.map_id,
        version: 0,
        nodes: route.path.clone(),
    }
}

pub(crate) fn route_to_trail(route: &Route, file_path: &RelativePath) -> Trail {
    let mut props = CommonAttributes::default();
    props.set_texture(None);
    props.set_trail_data(Some(file_path.clone()));
    Trail {
        map_id: route.map_id,
        category: route.category.clone(),
        parent: route.parent,
        guid: route.guid,
        props,
        dynamic: true,
        source_file_uuid: route.source_file_uuid,
    }
}
