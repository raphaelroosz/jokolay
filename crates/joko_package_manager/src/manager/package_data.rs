use std::collections::{BTreeMap, HashMap, HashSet};

use joko_component_models::{
    default_component_result, from_broadcast, from_data, to_data, Component, ComponentChannels,
    ComponentMessage, ComponentResult,
};
use joko_link_models::MumbleLink;
use joko_package_models::package::PackageImportReport;

use tracing::{error, info, info_span, trace};

use crate::{
    build_from_core, import_pack_from_zip_file_path, jokolay_to_editable_path,
    jokolay_to_extract_path,
    message::{MessageToPackageBack, MessageToPackageUI},
};
use miette::{IntoDiagnostic, Result};
use uuid::Uuid;

use crate::manager::pack::loaded::{LoadedPackData, PackTasks};

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
    pub root_path: std::path::PathBuf,
    #[allow(dead_code)]
    pub editable_path: std::path::PathBuf, //copy of the editable path in ui_configuration
    extract_path: std::path::PathBuf,
}

struct PackageDataChannels {
    subscription_mumblelink: tokio::sync::broadcast::Receiver<ComponentResult>,

    front_end_notifier: tokio::sync::mpsc::Sender<ComponentMessage>,
    front_end_receiver: tokio::sync::mpsc::Receiver<ComponentMessage>,
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
    channels: Option<PackageDataChannels>,

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
    pub fn new(root_path: &std::path::Path) -> Result<Self> {
        let marker_packs_path = jokolay_to_marker_path(root_path);
        //TODO: load configuration from disk (ui.toml)
        let editable_path = jokolay_to_editable_path(root_path)
            .to_str()
            .unwrap()
            .to_string();
        let state = PackageBackSharedState {
            choice_of_category_changed: false,
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
            channels: None,
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
        match msg {
            MessageToPackageBack::ActiveFiles(currently_used_files) => {
                trace!(
                    "Handling of MessageToPackageBack::ActiveFiles {}",
                    currently_used_files.len()
                );
                trace!(
                    "Handling of MessageToPackageBack::ActiveFiles {:?}",
                    currently_used_files
                );
                self.set_currently_used_files(currently_used_files);
                self.state.choice_of_category_changed = true;
            }
            MessageToPackageBack::CategoryActivationElementStatusChange(category_uuid, status) => {
                trace!("Handling of MessageToPackageBack::CategoryActivationElementStatusChange");
                self.category_set(category_uuid, status);
                self.state.choice_of_category_changed = true;
            }
            MessageToPackageBack::CategoryActivationBranchStatusChange(category_uuid, status) => {
                trace!("Handling of MessageToPackageBack::CategoryActivationBranchStatusChange");
                self.category_branch_set(category_uuid, status);
                self.state.choice_of_category_changed = true;
            }
            MessageToPackageBack::CategoryActivationStatusChanged => {
                trace!("Handling of MessageToPackageBack::CategoryActivationStatusChanged");
                self.state.choice_of_category_changed = true;
            }
            MessageToPackageBack::CategorySetAll(status) => {
                trace!(
                    "Handling of MessageToPackageBack::CategorySetAll {}",
                    status
                );
                self.category_set_all(status);
                self.state.choice_of_category_changed = true;
            }
            MessageToPackageBack::DeletePacks(to_delete) => {
                tracing::trace!("Handling of MessageToPackageBack::DeletePacks");

                let mut deleted = Vec::new();

                for pack_uuid in to_delete {
                    if let Some(pack) = self.packs.remove(&pack_uuid) {
                        let target = self.marker_packs_path.join(&pack.name);
                        if let Err(e) = std::fs::remove_dir_all(target) {
                            error!(?e, pack.name, "failed to remove pack");
                        } else {
                            info!("deleted marker pack: {}", pack.name);
                            deleted.push(pack_uuid);
                        }
                    }
                }
                let channels = self.channels.as_mut().unwrap();
                let _ = channels
                    .front_end_notifier
                    .blocking_send(to_data(MessageToPackageUI::DeletedPacks(deleted)));
            }
            MessageToPackageBack::ImportPack(file_path) => {
                tracing::trace!("Handling of MessageToPackageBack::ImportPack");
                let channels = self.channels.as_mut().unwrap();
                let _ = channels
                    .front_end_notifier
                    .blocking_send(to_data(MessageToPackageUI::NbTasksRunning(1)));
                let start = std::time::SystemTime::now();
                let result = import_pack_from_zip_file_path(file_path, &self.state.extract_path);
                let elaspsed = start.elapsed().unwrap_or_default();
                tracing::info!(
                    "Loading of taco package from disk took {} ms",
                    elaspsed.as_millis()
                );
                match result {
                    Ok((file_name, pack)) => {
                        let _ = channels.front_end_notifier.blocking_send(to_data(
                            MessageToPackageUI::ImportedPack(file_name, pack),
                        ));
                    }
                    Err(e) => {
                        let _ = channels
                            .front_end_notifier
                            .blocking_send(to_data(MessageToPackageUI::ImportFailure(e)));
                    }
                }
                let _ = channels
                    .front_end_notifier
                    .blocking_send(to_data(MessageToPackageUI::NbTasksRunning(0)));
            }
            MessageToPackageBack::ReloadPack => {
                unimplemented!(
                    "Handling of MessageToPackageBack::ReloadPack has not been implemented yet"
                );
            }
            MessageToPackageBack::SavePack(name, pack) => {
                tracing::trace!("Handling of MessageToPackageBack::SavePack");
                trace!("save in {:?}", self.marker_packs_path);

                /*let std_file = std::fs::OpenOptions::new()
                    .open(&self.marker_packs_path)
                    .unwrap();
                let marker_packs_dir = cap_std::fs_utf8::Dir::from_std_file(std_file);*/
                let name = name.as_str();
                let pack_path = self.marker_packs_path.join(name);

                if pack_path.exists() {
                    match std::fs::remove_dir_all(pack_path.clone()).into_diagnostic() {
                        Ok(_) => {}
                        Err(e) => {
                            error!(?e, "failed to delete already existing marker pack");
                        }
                    }
                }
                if let Err(e) = std::fs::create_dir_all(pack_path.clone()) {
                    error!(?e, "failed to create directory for pack");
                }

                let (data_pack, mut texture_pack, mut report) =
                    build_from_core(name.to_string(), pack_path, pack);
                tracing::trace!("Package loaded into data and texture");
                let uuid_of_insertion = self.save(data_pack, report.clone());
                report.uuid = uuid_of_insertion;
                texture_pack.uuid = uuid_of_insertion;
                let channels = self.channels.as_mut().unwrap();
                let _ = channels.front_end_notifier.blocking_send(to_data(
                    MessageToPackageUI::LoadedPack(texture_pack, report),
                ));
            }
            #[allow(unreachable_patterns)]
            _ => {
                unimplemented!("Handling MessageToPackageBack has not been implemented yet");
            }
        }
    }

    pub fn _tick(&mut self, link: &Option<MumbleLink>) {
        if let Some(link) = link {
            //TODO: how to save/load the active files ?
            let mut have_used_files_list_changed = false;
            let map_changed = self.current_map_id != link.map_id;
            self.current_map_id = link.map_id;
            trace!(
                "PackageDataManager::tick map id is: {}",
                self.current_map_id
            );
            let mut currently_used_files: BTreeMap<Uuid, bool> = Default::default();
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
            trace!(
                "currently_used_files: {} {:?}",
                currently_used_files.len(),
                currently_used_files
            );
            let tasks = &self.tasks;
            if map_changed || have_used_files_list_changed || self.state.choice_of_category_changed
            {
                let mut categories_and_elements_to_be_loaded: HashSet<Uuid> = Default::default();
                {
                    let channels = self.channels.as_mut().unwrap();
                    let _ = channels
                        .front_end_notifier
                        .blocking_send(to_data(MessageToPackageUI::TextureBegin));
                }
                for pack in self.packs.values_mut() {
                    let span_guard = info_span!("Updating package status").entered();
                    let channels = self.channels.as_mut().unwrap();
                    let _ = channels
                        .front_end_notifier
                        .blocking_send(to_data(MessageToPackageUI::NbTasksRunning(tasks.count())));
                    tasks.save_data(pack, pack.is_dirty());
                    pack.tick(
                        &channels.front_end_notifier,
                        link,
                        &currently_used_files,
                        tasks,
                        &mut categories_and_elements_to_be_loaded,
                    );
                    std::mem::drop(span_guard);
                }

                self.get_active_elements_parents(categories_and_elements_to_be_loaded);

                //there is no point in sending a new list if nothing changed

                let channels = self.channels.as_mut().unwrap();
                let _ = channels.front_end_notifier.blocking_send(to_data(
                    MessageToPackageUI::CurrentlyUsedFiles(currently_used_files.clone()),
                ));
                self.currently_used_files = currently_used_files;

                let _ = channels.front_end_notifier.blocking_send(to_data(
                    MessageToPackageUI::ActiveElements(self.loaded_elements.clone()),
                ));
                let _ = channels
                    .front_end_notifier
                    .blocking_send(to_data(MessageToPackageUI::TextureSwapChain));
            }
            self.state.choice_of_category_changed = false;
        } else {
            trace!("PackageDataManager::tick no link")
        }
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
        self.tasks.save_report(data_pack.path.clone(), report, true);
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
        assert!(
            self.channels.is_some(),
            "channels must be initialized before interacting with component."
        );
        once::assert_has_not_been_called!("Early load must happen only once");
        trace!("Load all packages");
        let channels = self.channels.as_mut().unwrap();
        // Called only once at application start.
        let _ = channels
            .front_end_notifier
            .blocking_send(to_data(MessageToPackageUI::NbTasksRunning(1)));
        self.tasks.load_all_packs(self.state.root_path.clone());
        if let Ok((data_packages, texture_packages, report_packages)) =
            self.tasks.wait_for_load_all_packs()
        {
            for (uuid, data_pack) in data_packages {
                self.packs.insert(uuid, data_pack);
            }
            for ((_, texture_pack), (_, report)) in
                std::iter::zip(texture_packages, report_packages)
            {
                trace!("load_all notify front of a valid loaded package");
                let _ = channels.front_end_notifier.blocking_send(to_data(
                    MessageToPackageUI::LoadedPack(texture_pack, report),
                ));
            }

            let _ = channels
                .front_end_notifier
                .blocking_send(to_data(MessageToPackageUI::NbTasksRunning(0)));
        }
        let _ = channels
            .front_end_notifier
            .blocking_send(to_data(MessageToPackageUI::FirstLoadDone));
    }
}

impl Component for PackageDataManager {
    fn init(&mut self) {
        self.load_all();
    }

    fn flush_all_messages(&mut self) {
        assert!(
            self.channels.is_some(),
            "channels must be initialized before interacting with component."
        );
        tracing::trace!(
            "choice_of_category_changed: {}",
            self.state.choice_of_category_changed
        );

        let channels = self.channels.as_mut().unwrap();
        //println!("PackageDataManager: nb messages to read: {}", channels.front_end_receiver.len());
        let mut messages = Vec::new();
        while let Ok(msg) = channels.front_end_receiver.try_recv() {
            messages.push(from_data(&msg));
        }
        for msg in messages {
            self.handle_message(msg);
        }
    }
    fn bind(&mut self, mut channels: ComponentChannels) {
        let (front_end_notifier, front_end_receiver) = channels.peers.remove(&0).unwrap();
        let channels = PackageDataChannels {
            subscription_mumblelink: channels.requirements.remove(&1).unwrap(),
            front_end_notifier,
            front_end_receiver,
        };
        self.channels = Some(channels);
    }
    fn tick(&mut self, _latest_time: f64) -> ComponentResult {
        assert!(
            self.channels.is_some(),
            "channels must be initialized before interacting with component."
        );
        let channels = self.channels.as_mut().unwrap();
        //trace!("blocking waiting for subscription_mumblelink {}", channels.subscription_mumblelink.len());
        let raw_mlr = channels.subscription_mumblelink.try_recv().unwrap();
        let mumble_link_result: Option<MumbleLink> = from_broadcast(&raw_mlr);
        //trace!("subscription_mumblelink provided data");
        self._tick(&mumble_link_result);
        default_component_result()
    }
    fn notify(&self) -> Vec<&str> {
        vec![]
    }
    fn peers(&self) -> Vec<&str> {
        vec!["ui:jokolay_package_manager"]
    }
    fn requirements(&self) -> Vec<&str> {
        vec!["back:mumble_link"]
    }
    fn accept_notifications(&self) -> bool {
        false
    }
}
