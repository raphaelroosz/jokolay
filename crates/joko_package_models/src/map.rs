use uuid::Uuid;
use indexmap::IndexMap;
use crate::marker::Marker;
use crate::route::Route;
use crate::trail::Trail;

#[derive(Default, Debug, Clone)]
pub struct MapData {
    pub markers: IndexMap<Uuid, Marker>,
    pub routes: IndexMap<Uuid, Route>,
    pub trails: IndexMap<Uuid, Trail>,
}

