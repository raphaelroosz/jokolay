use std::sync::Arc;
use std::collections::{BTreeMap, HashSet};

use uuid::Uuid;

use glam::{Vec2, Vec3};

use jokolink::MumbleLink;
use joko_core::RelativePath;

use crate::{pack::{CommonAttributes, PackCore}, LoadedPackTexture};


#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct MarkerVertex {
    pub position: Vec3,
    pub alpha: f32,
    pub texture_coordinates: Vec2,
    pub fade_near_far: Vec2,
    pub color: [u8; 4],
}

#[derive(Debug)]
pub struct MarkerObject {
    /// The six vertices that make up the marker quad
    pub vertices: [MarkerVertex; 6],
    /// The (managed) texture id from egui data
    pub texture: u64,
    /// The distance from camera
    /// As markers have transparency, we need to render them from far -> near order
    /// So, we will sort them using this distance just before rendering
    pub distance: f32,
}

#[derive(Debug, Clone)]
pub struct TrailObject {
    pub vertices: Arc<[MarkerVertex]>,
    pub texture: u64,
}

pub enum BackToUIMessage {
    ActiveElements(HashSet<Uuid>),//list of all elements that are loaded for current map
    CurrentlyUsedFiles(BTreeMap<String, bool>),//when there is a change in map or anything else, the list of files is sent to ui for display
    LoadedPack(LoadedPackTexture),//push a loaded pack to UI
    DeletedPacks(Vec<Uuid>),//push a deleted set of packs to UI
    ImportedPack(String, PackCore),
    ImportFailure(miette::Report),
    MarkerTexture(Uuid, RelativePath, Uuid, Vec3, CommonAttributes),
    MumbleLink(Option<MumbleLink>),
    MumbleLinkChanged,//tell there is a need to resize
    NbTasksRunning(i32),//tell the number of taks running in background
    PackageActiveElements(Uuid, HashSet<Uuid>),// first is the package reference, second is the list of active elements in the package.
    TextureSwapChain,// The list of texture to load was changed, will be soon followed by a RenderSwapChain
    TrailTexture(Uuid, RelativePath, Uuid, CommonAttributes),
}

pub enum UIToBackMessage {
    ActiveFiles(BTreeMap<String, bool>),//when there is a change of files activated, send whole list to data for save.
    CategoryActivationElementStatusChange(Uuid, bool),//sent each time there is a category whose activation status has been changed. With uuid being the reference of the category and bool the status.
    CategoryActivationBranchStatusChange(Uuid, bool),//same, for a whole branch
    CategoryActivationStatusChanged,//something happened that needs to reload the whole set
    CategorySetAll(bool),//signal all categories should be now at this status
    DeletePacks(Vec<Uuid>),//uuid of the pack to delete
    ImportPack(std::path::PathBuf),
    ReloadPack,
    SavePack(String, PackCore),
}

pub enum UIToUIMessage {
    BulkMarkerObject(Vec<MarkerObject>),
    BulkTrailObject(Vec<TrailObject>),
    //Present,// a render loop is finished and we can present it
    MarkerObject(MarkerObject),
    RenderSwapChain,// The list of elements to display was changed
    TrailObject(TrailObject),
}

