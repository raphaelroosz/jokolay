use std::collections::{BTreeMap, HashSet};

use joko_package_models::{
    attributes::CommonAttributes,
    package::{PackCore, PackageImportReport},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use joko_core::{serde_glam::Vec3, RelativePath};

use crate::LoadedPackTexture;

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageToPackageUI {
    ActiveElements(HashSet<Uuid>), //list of all elements that are loaded for current map
    CurrentlyUsedFiles(BTreeMap<Uuid, bool>), //when there is a change in map or anything else, the list of files is sent to ui for display
    LoadedPack(LoadedPackTexture, PackageImportReport), //push a loaded pack to UI
    DeletedPacks(Vec<Uuid>),                  //push a deleted set of packs to UI
    FirstLoadDone,
    ImportedPack(String, PackCore),
    ImportFailure(String),
    MarkerTexture(Uuid, RelativePath, Uuid, Vec3, CommonAttributes),
    NbTasksRunning(i32), //tell the number of taks running in background
    PackageActiveElements(Uuid, HashSet<Uuid>), // first is the package reference, second is the list of active elements in the package.
    TextureBegin,                               // start to produce new set of textures
    TextureSwapChain, // The list of texture to load was changed, will be soon followed by a RenderSwapChain
    TrailTexture(Uuid, RelativePath, Uuid, CommonAttributes),
}

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageToPackageBack {
    ActiveFiles(BTreeMap<Uuid, bool>), //when there is a change of files activated, send whole list to data for save.
    CategoryActivationElementStatusChange(Uuid, bool), //sent each time there is a category whose activation status has been changed. With uuid being the reference of the category and bool the status.
    CategoryActivationBranchStatusChange(Uuid, bool),  //same, for a whole branch
    CategoryActivationStatusChanged, //something happened that needs to reload the whole set
    CategorySetAll(bool),            //signal all categories should be now at this status
    DeletePacks(Vec<Uuid>),          //uuid of the pack to delete
    ImportPack(std::path::PathBuf),
    ReloadPack,
    SavePack(String, PackCore),
}