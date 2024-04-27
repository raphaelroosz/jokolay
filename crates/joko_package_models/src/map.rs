use crate::marker::Marker;
use crate::route::Route;
use crate::trail::Trail;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct MapData {
    pub markers: IndexMap<Uuid, Marker>,
    pub routes: IndexMap<Uuid, Route>,
    pub trails: IndexMap<Uuid, Trail>,
}
