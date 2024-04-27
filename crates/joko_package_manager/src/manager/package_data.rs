use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use cap_std::fs_utf8::Dir;
use joko_component_models::ComponentDataExchange;
use joko_package_models::package::PackageImportReport;

use tracing::{error, info, info_span, trace};

use crate::{
    build_from_core, import_pack_from_zip_file_path, jokolay_to_editable_path,
    jokolay_to_extract_path, message::MessageToPackageBack,
};
use joko_link_models::{MumbleLink, MumbleLinkSharedState};
use miette::{IntoDiagnostic, Result};
use uuid::Uuid;

use crate::manager::pack::loaded::{LoadedPackData, PackTasks};
use crate::message::MessageToPackageUI;

use super::pack::loaded::jokolay_to_marker_path;

pub const PACKAGE_MANAGER_DIRECTORY_NAME: &str = "marker_manager"; //name kept for compatibility purpose
pub const PACKAGES_DIRECTORY_NAME: &str = "packs"; //name kept for compatibility purpose
pub const EXTRACT_DIRECTORY_NAME: &str = "_work"; //working dir where a package is extracted before reading
pub const EDITABLE_PACKAGE_NAME: &str = "editable"; //package automatically created and always imported as an overwrite
pub const LOCAL_EXPANDED_PACKAGE_NAME: &str = "_local_expanded"; //result of import of the editable package
                                                                 // pub const MARKER_MANAGER_CONFIG_NAME: &str = "marker_manager_config.json";

#[derive(Clone)]
pub struct PackageBackSharedState {
    choice_of_category_changed: bool, //Meant as an optimisation to only update when there is a change in UI
    pub root_dir: Arc<Dir>,
    pub root_path: std::path::PathBuf,
    #[allow(dead_code)]
    pub editable_path: std::path::PathBuf, //copy of the editable path in ui_configuration
    extract_path: std::path::PathBuf,
}

/// It manage everything that has to do with marker packs.
/// 1. imports, loads, saves and exports marker packs.
/// 2. maintains the categories selection data for every pack
/// 3. contains activation data globally and per character
/// 4. When we load into a map, it filters the markers and runs the logic every frame
///     1. If a marker needs to be activated (based on player position or whatever)
///     2. marker needs to be drawn
///     3. marker's texture is uploaded or being uploaded? if not ready, we will upload or use a temporary "loading" texture
///     4. render that marker use joko_render  

#[must_use]
pub struct PackageDataManager {
    /// marker manager directory. not useful yet, but in future we could be using this to store config files etc..
    //_marker_manager_dir: Arc<Dir>,
    /// packs directory which contains marker packs. each directory inside pack directory is an individual marker pack.
    /// The name of the child directory is the name of the pack
    //pub marker_packs_dir: Arc<Dir>,
    pub marker_packs_path: std::path::PathBuf,
    /// These are the marker packs
    /// The key is the name of the pack
    /// The value is a loaded pack that contains additional data for live marker packs like what needs to be saved or category selections etc..
    pub packs: BTreeMap<Uuid, LoadedPackData>,
    tasks: PackTasks,
    current_map_id: u32,
    /// This is the interval in number of seconds when we check if any of the packs need to be saved due to changes.
    /// This allows us to avoid saving the pack too often.
    pub save_interval: f64,

    pub currently_used_files: BTreeMap<Uuid, bool>,
    parents: HashMap<Uuid, Uuid>,
    loaded_elements: HashSet<Uuid>,
    channel_receiver: tokio::sync::mpsc::Receiver<ComponentDataExchange>,
    channel_sender: tokio::sync::mpsc::Sender<ComponentDataExchange>,
    pub state: PackageBackSharedState,
}

impl PackageDataManager {
    /// Creates a new instance of [MarkerManager].
    /// 1. It opens the marker manager directory
    /// 2. loads its configuration
    /// 3. opens the packs directory
    /// 4. loads all the packs
    /// 5. loads all the activation data
    /// 6. returns self
    pub fn new(
        root_dir: Arc<Dir>,
        root_path: &std::path::Path,
        channel_receiver: tokio::sync::mpsc::Receiver<ComponentDataExchange>,
        channel_sender: tokio::sync::mpsc::Sender<ComponentDataExchange>,
    ) -> Result<Self> {
        let marker_packs_path = jokolay_to_marker_path(root_path);
        //TODO: load configuration from disk (ui.toml)
        let editable_path = jokolay_to_editable_path(root_path)
            .to_str()
            .unwrap()
            .to_string();
        let state = PackageBackSharedState {
            choice_of_category_changed: false,
            root_dir,
            root_path: root_path.to_owned(),
            editable_path: std::path::PathBuf::from(editable_path),
            extract_path: jokolay_to_extract_path(root_path),
        };
        Ok(Self {
            packs: Default::default(),
            tasks: PackTasks::new(),
            //marker_packs_dir: Arc::new(marker_packs_dir),
            marker_packs_path,
            current_map_id: 0,
            save_interval: 0.0,
            currently_used_files: Default::default(),
            parents: Default::default(),
            loaded_elements: Default::default(),
            channel_sender,
            channel_receiver,
            state,
        })
    }

    pub fn set_currently_used_files(&mut self, currently_used_files: BTreeMap<Uuid, bool>) {
        self.currently_used_files = currently_used_files;
    }

    pub fn category_set(&mut self, uuid: Uuid, status: bool) {
        for pack in self.packs.values_mut() {
            if pack.category_set(uuid, status) {
                break;
            }
        }
    }

    pub fn category_branch_set(&mut self, uuid: Uuid, status: bool) {
        for pack in self.packs.values_mut() {
            if pack.category_branch_set(uuid, status) {
                break;
            }
        }
    }

    pub fn category_set_all(&mut self, status: bool) {
        for pack in self.packs.values_mut() {
            pack.category_set_all(status);
        }
    }

    pub fn register(&mut self, element: Uuid, parent: Uuid) {
        self.parents.insert(element, parent);
    }
    pub fn get_parent(&self, element: &Uuid) -> Option<&Uuid> {
        self.parents.get(element)
    }
    pub fn get_parents<'a, I>(&self, input: I) -> HashSet<Uuid>
    where
        I: Iterator<Item = &'a Uuid>,
    {
        let iter = input.into_iter();
        let mut result: HashSet<Uuid> = HashSet::new();
        let mut current_generation: Vec<Uuid> = Vec::new();
        for elt in iter {
            current_generation.push(*elt)
        }
        //info!("starts with {}", current_generation.len());
        loop {
            if current_generation.is_empty() {
                //info!("ends with {}", result.len());
                return result;
            }
            let mut next_gen: Vec<Uuid> = Vec::new();
            for elt in current_generation.iter() {
                if let Some(p) = self.get_parent(elt) {
                    if result.contains(p) {
                        //avoid duplicate, redundancy or loop
                        continue;
                    }
                    next_gen.push(*p);
                }
            }
            let to_insert = std::mem::replace(&mut current_generation, next_gen);
            result.extend(to_insert);
        }
        #[allow(unreachable_code)] // sillyness of some tools
        {
            unreachable!("The loop should always return")
        }
    }

    pub fn get_active_elements_parents(
        &mut self,
        categories_and_elements_to_be_loaded: HashSet<Uuid>,
    ) {
        trace!(
            "There are {} active elements",
            categories_and_elements_to_be_loaded.len()
        );

        //first merge the parents to iterate overit
        let mut parents: HashMap<Uuid, Uuid> = Default::default();
        for pack in self.packs.values_mut() {
            parents.extend(pack.entities_parents.clone());
        }
        self.parents = parents;
        //then climb up the tree of parent's categories
        self.loaded_elements = self.get_parents(categories_and_elements_to_be_loaded.iter());
    }

    fn handle_message(&mut self, msg: MessageToPackageBack) {
        //let (b2u_sender, _) = package_manager.channels();
        match msg {
            MessageToPackageBack::ActiveFiles(currently_used_files) => {
                tracing::trace!("Handling of MessageToPackageBack::ActiveFiles");
                self.set_currently_used_files(currently_used_files);
                self.state.choice_of_category_changed = true;
            }
            MessageToPackageBack::CategoryActivationElementStatusChange(category_uuid, status) => {
                tracing::trace!(
                    "Handling of MessageToPackageBack::CategoryActivationElementStatusChange"
                );
                self.category_set(category_uuid, status);
            }
            MessageToPackageBack::CategoryActivationBranchStatusChange(category_uuid, status) => {
                tracing::trace!(
                    "Handling of MessageToPackageBack::CategoryActivationBranchStatusChange"
                );
                self.category_branch_set(category_uuid, status);
            }
            MessageToPackageBack::CategoryActivationStatusChanged => {
                tracing::trace!(
                    "Handling of MessageToPackageBack::CategoryActivationStatusChanged"
                );
                self.state.choice_of_category_changed = true;
            }
            MessageToPackageBack::CategorySetAll(status) => {
                tracing::trace!("Handling of MessageToPackageBack::CategorySetAll");
                self.category_set_all(status);
                self.state.choice_of_category_changed = true;
            }
            MessageToPackageBack::DeletePacks(to_delete) => {
                tracing::trace!("Handling of MessageToPackageBack::DeletePacks");
                let std_file = std::fs::OpenOptions::new()
                    .open(&self.marker_packs_path)
                    .or(Err("Could not open file"))
                    .unwrap();
                let marker_packs_dir = cap_std::fs_utf8::Dir::from_std_file(std_file);
                let mut deleted = Vec::new();

                for pack_uuid in to_delete {
                    if let Some(pack) = self.packs.remove(&pack_uuid) {
                        if let Err(e) = marker_packs_dir.remove_dir_all(&pack.name) {
                            error!(?e, pack.name, "failed to remove pack");
                        } else {
                            info!("deleted marker pack: {}", pack.name);
                            deleted.push(pack_uuid);
                        }
                    }
                }
                let _ = self
                    .channel_sender
                    .blocking_send(MessageToPackageUI::DeletedPacks(deleted).into());
            }
            MessageToPackageBack::ImportPack(file_path) => {
                tracing::trace!("Handling of MessageToPackageBack::ImportPack");
                let _ = self
                    .channel_sender
                    .blocking_send(MessageToPackageUI::NbTasksRunning(1).into());
                let start = std::time::SystemTime::now();
                let result = import_pack_from_zip_file_path(file_path, &self.state.extract_path);
                let elaspsed = start.elapsed().unwrap_or_default();
                tracing::info!(
                    "Loading of taco package from disk took {} ms",
                    elaspsed.as_millis()
                );
                match result {
                    Ok((file_name, pack)) => {
                        let _ = self.channel_sender.blocking_send(
                            MessageToPackageUI::ImportedPack(file_name, pack).into(),
                        );
                    }
                    Err(e) => {
                        let _ = self
                            .channel_sender
                            .blocking_send(MessageToPackageUI::ImportFailure(e).into());
                    }
                }
                let _ = self
                    .channel_sender
                    .blocking_send(MessageToPackageUI::NbTasksRunning(0).into());
            }
            MessageToPackageBack::ReloadPack => {
                unimplemented!(
                    "Handling of MessageToPackageBack::ReloadPack has not been implemented yet"
                );
            }
            MessageToPackageBack::SavePack(name, pack) => {
                tracing::trace!("Handling of MessageToPackageBack::SavePack");
                let std_file = std::fs::OpenOptions::new()
                    .open(&self.marker_packs_path)
                    .or(Err("Could not open file"))
                    .unwrap();
                let marker_packs_dir = cap_std::fs_utf8::Dir::from_std_file(std_file);
                let name = name.as_str();
                if marker_packs_dir.exists(name) {
                    match marker_packs_dir.remove_dir_all(name).into_diagnostic() {
                        Ok(_) => {}
                        Err(e) => {
                            error!(?e, "failed to delete already existing marker pack");
                        }
                    }
                }
                if let Err(e) = marker_packs_dir.create_dir_all(name) {
                    error!(?e, "failed to create directory for pack");
                }
                match marker_packs_dir.open_dir(name) {
                    Ok(dir) => {
                        let pack_path = self.marker_packs_path.join(name);
                        let (data_pack, mut texture_pack, mut report) =
                            build_from_core(name.to_string(), dir.into(), pack_path, pack);
                        tracing::trace!("Package loaded into data and texture");
                        let uuid_of_insertion = self.save(data_pack, report.clone());
                        report.uuid = uuid_of_insertion;
                        texture_pack.uuid = uuid_of_insertion;
                        let _ = self.channel_sender.blocking_send(
                            MessageToPackageUI::LoadedPack(texture_pack, report).into(),
                        );
                    }
                    Err(e) => {
                        error!(
                            ?e,
                            "failed to open marker pack directory to save pack {:?} {}",
                            self.marker_packs_path,
                            name
                        );
                    }
                };
            }
            #[allow(unreachable_patterns)]
            _ => {
                unimplemented!("Handling MessageToPackageBack has not been implemented yet");
            }
        }
    }

    pub fn flush_all_messages(&mut self) -> PackageBackSharedState {
        tracing::trace!(
            "choice_of_category_changed: {}",
            self.state.choice_of_category_changed
        );

        let mut messages = Vec::new();
        while let Ok(msg) = self.channel_receiver.try_recv() {
            let msg = bincode::deserialize(&msg).unwrap();
            messages.push(msg);
        }
        for msg in messages {
            self.handle_message(msg);
        }
        self.state.clone()
    }

    pub fn tick(
        &mut self,
        loop_index: u128,
        ms: &MumbleLinkSharedState,
        link: Option<&MumbleLink>,
    ) {
        let mut currently_used_files: BTreeMap<Uuid, bool> = Default::default();
        let mut categories_and_elements_to_be_loaded: HashSet<Uuid> = Default::default();

        let link = if ms.read_ui_link {
            ms.copy_of_ui_link.as_ref()
        } else {
            link
        };

        if let Some(link) = link {
            //TODO: how to save/load the active files ?
            let mut have_used_files_list_changed = false;
            let map_changed = self.current_map_id != link.map_id;
            self.current_map_id = link.map_id;
            for pack in self.packs.values_mut() {
                if let Some(current_map) = pack.maps.get(&link.map_id) {
                    for marker in current_map.markers.values() {
                        if let Some(is_active) = pack.source_files.get(&marker.source_file_uuid) {
                            currently_used_files.insert(
                                marker.source_file_uuid,
                                *self
                                    .currently_used_files
                                    .get(&marker.source_file_uuid)
                                    .unwrap_or_else(|| {
                                        have_used_files_list_changed = true;
                                        is_active
                                    }),
                            );
                        }
                    }
                    for trail in current_map.trails.values() {
                        if let Some(is_active) = pack.source_files.get(&trail.source_file_uuid) {
                            currently_used_files.insert(
                                trail.source_file_uuid,
                                *self
                                    .currently_used_files
                                    .get(&trail.source_file_uuid)
                                    .unwrap_or_else(|| {
                                        have_used_files_list_changed = true;
                                        is_active
                                    }),
                            );
                        }
                    }
                }
            }
            let tasks = &self.tasks;
            for pack in self.packs.values_mut() {
                let span_guard = info_span!("Updating package status").entered();
                let _ = self
                    .channel_sender
                    .blocking_send(MessageToPackageUI::NbTasksRunning(tasks.count()).into());
                tasks.save_data(pack, pack.is_dirty());
                pack.tick(
                    &self.channel_sender,
                    loop_index,
                    link,
                    &currently_used_files,
                    have_used_files_list_changed || self.state.choice_of_category_changed,
                    map_changed,
                    tasks,
                    &mut categories_and_elements_to_be_loaded,
                );
                std::mem::drop(span_guard);
            }
            if map_changed {
                self.get_active_elements_parents(categories_and_elements_to_be_loaded);
                let _ = self.channel_sender.blocking_send(
                    MessageToPackageUI::ActiveElements(self.loaded_elements.clone()).into(),
                );
            }
            if map_changed || have_used_files_list_changed || self.state.choice_of_category_changed
            {
                //there is no point in sending a new list if nothing changed
                let _ = self.channel_sender.blocking_send(
                    MessageToPackageUI::CurrentlyUsedFiles(currently_used_files.clone()).into(),
                );
                self.currently_used_files = currently_used_files;
                let _ = self
                    .channel_sender
                    .blocking_send(MessageToPackageUI::TextureSwapChain.into());
            }
        }
        self.state.choice_of_category_changed = false;
    }

    fn delete_packs(&mut self, to_delete: Vec<Uuid>) {
        for uuid in to_delete {
            self.packs.remove(&uuid);
        }
    }
    pub fn save(&mut self, mut data_pack: LoadedPackData, report: PackageImportReport) -> Uuid {
        let mut to_delete: Vec<Uuid> = Vec::new();
        for (uuid, pack) in self.packs.iter() {
            if pack.name == data_pack.name {
                to_delete.push(*uuid);
            }
        }
        self.delete_packs(to_delete);
        self.tasks
            .save_report(Arc::clone(&data_pack.dir), report, true);
        self.tasks.save_data(&mut data_pack, true);
        let mut uuid_to_insert = data_pack.uuid;
        while self.packs.contains_key(&uuid_to_insert) {
            //collision avoidance
            trace!(
                "Uuid collision detected for {} for package {}",
                uuid_to_insert,
                data_pack.name
            );
            uuid_to_insert = Uuid::new_v4();
        }
        data_pack.uuid = uuid_to_insert;
        self.packs.insert(uuid_to_insert, data_pack);
        uuid_to_insert
    }

    pub fn load_all(&mut self) {
        once::assert_has_not_been_called!("Early load must happen only once");
        // Called only once at application start.
        let _ = self
            .channel_sender
            .blocking_send(MessageToPackageUI::NbTasksRunning(1).into());
        self.tasks.load_all_packs(
            Arc::clone(&self.state.root_dir),
            self.state.root_path.clone(),
        );
        if let Ok((data_packages, texture_packages, report_packages)) =
            self.tasks.wait_for_load_all_packs()
        {
            for (uuid, data_pack) in data_packages {
                self.packs.insert(uuid, data_pack);
            }
            for ((_, texture_pack), (_, report)) in
                std::iter::zip(texture_packages, report_packages)
            {
                let _ = self
                    .channel_sender
                    .blocking_send(MessageToPackageUI::LoadedPack(texture_pack, report).into());
            }

            let _ = self
                .channel_sender
                .blocking_send(MessageToPackageUI::NbTasksRunning(0).into());
        }
        let _ = self
            .channel_sender
            .blocking_send(MessageToPackageUI::FirstLoadDone.into());
    }
}
