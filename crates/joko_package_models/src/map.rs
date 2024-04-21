use crate::marker::Marker;
use crate::route::Route;
use crate::trail::Trail;
use indexmap::IndexMap;
use uuid::Uuid;

#[derive(Default, Debug, Clone)]
pub struct MapData {
    pub markers: IndexMap<Uuid, Marker>,
    pub routes: IndexMap<Uuid, Route>,
    pub trails: IndexMap<Uuid, Trail>,
}
