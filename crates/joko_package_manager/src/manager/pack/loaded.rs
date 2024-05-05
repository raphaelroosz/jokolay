use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

use joko_component_models::{to_data, ComponentMessage};
use joko_package_models::{
    attributes::{Behavior, CommonAttributes},
    category::Category,
    map::MapData,
    package::{PackCore, PackageImportReport},
    trail::TBin,
};
use ordered_hash_map::OrderedHashMap;

use cap_std::fs_utf8::Dir;
use egui::{ColorImage, TextureHandle};
use image::EncodableLayout;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, info_span, trace};
use uuid::Uuid;

use crate::message::MessageToPackageUI;
use crate::{
    io::{load_pack_core_from_normalized_folder, save_pack_data_to_dir, save_pack_texture_to_dir},
    manager::{
        pack::{category_selection::SelectedCategoryManager, file_selection::SelectedFileManager},
        package_data::EXTRACT_DIRECTORY_NAME,
    },
    message::MessageToPackageBack,
};
use joko_core::{
    serde_glam::Vec3,
    task::{AsyncTask, AsyncTaskGuard},
    RelativePath,
};
use joko_link_models::MumbleLink;
use joko_render_models::{messages::MessageToRenderer, trail::TrailObject};
use miette::{Context, IntoDiagnostic, Result};

use super::activation::{ActivationData, ActivationType};
use super::active::{ActiveMarker, ActiveTrail, CurrentMapData};
use crate::manager::pack::category_selection::CategorySelection;
use crate::manager::package_data::{
    EDITABLE_PACKAGE_NAME, LOCAL_EXPANDED_PACKAGE_NAME, PACKAGES_DIRECTORY_NAME,
    PACKAGE_MANAGER_DIRECTORY_NAME,
};

type ImportAllTriplet = (
    BTreeMap<Uuid, LoadedPackData>,
    BTreeMap<Uuid, LoadedPackTexture>,
    BTreeMap<Uuid, PackageImportReport>,
);
type ImportTriplet = (LoadedPackData, LoadedPackTexture, PackageImportReport);

//TODO: separate in front and back tasks
pub(crate) struct PackTasks {
    //an object that can handle such tasks should be passed as argument of any function that may required an async action
    save_texture_task: AsyncTask<LoadedPackTexture, Result<(), String>>,
    save_data_task: AsyncTask<LoadedPackData, Result<(), String>>,
    save_report_task: AsyncTask<(PathBuf, PackageImportReport), Result<(), String>>,
    load_all_packs_task:
        AsyncTask<(Arc<Dir>, std::path::PathBuf), Result<ImportAllTriplet, String>>,
}

//TOOD: move the LoadedPackData & LoadedPackTexture to joko_package_models ? The problem is about the messages to be sent. Where to put them ? and at the cost of which dependancy ?
#[derive(Clone)]
pub struct LoadedPackData {
    pub name: String,
    pub uuid: Uuid,
    pub path: PathBuf,
    /// The actual xml pack.
    //pub core: PackCore,
    pub categories: OrderedHashMap<Uuid, Category>,
    pub all_categories: HashMap<String, Uuid>,
    pub source_files: BTreeMap<Uuid, bool>, //TODO: have a reference containing pack name and maybe even path inside the package
    pub maps: HashMap<u32, MapData>,
    selected_files: BTreeMap<Uuid, bool>,
    _is_dirty: bool, //there was an edition in the package itself

    // loca copy in the data side of what is exposed in UI
    selectable_categories: OrderedHashMap<String, CategorySelection>,
    pub entities_parents: HashMap<Uuid, Uuid>,
    activation_data: ActivationData,
    active_elements: HashSet<Uuid>, //keep track of which elements are active
}

#[derive(Clone, Serialize, Deserialize)]
pub struct LoadedPackTexture {
    //TODO: there is a need for a late loading of texture to avoid transmitting them (serialize)
    pub name: String,
    pub uuid: Uuid,
    /// The directory inside which the pack data is stored
    /// There should be a subdirectory called `core` which stores the pack core
    /// Files related to Jokolay thought will have to be stored directly inside this directory, to keep the xml subdirectory clean.
    /// eg: Active categories, activation data etc..
    //pub dir: Arc<Dir>,
    pub path: std::path::PathBuf,
    pub source_files: BTreeMap<Uuid, bool>,
    pub tbins: HashMap<RelativePath, TBin>,
    pub textures: HashMap<RelativePath, Vec<u8>>,

    /// The selection of categories which are "enabled" and markers belonging to these may be rendered
    selectable_categories: OrderedHashMap<String, CategorySelection>,
    #[serde(skip)]
    current_map_data: CurrentMapData,
    activation_data: ActivationData,
    //active_elements: HashSet<Uuid>, //which are the active elements (loaded)
    _is_dirty: bool,
}

impl PackTasks {
    pub fn new() -> Self {
        Self {
            save_texture_task: AsyncTaskGuard::new(PackTasks::async_save_texture),
            save_data_task: AsyncTaskGuard::new(PackTasks::async_save_data),
            save_report_task: AsyncTaskGuard::new(PackTasks::async_save_report),
            load_all_packs_task: AsyncTaskGuard::new(load_all_from_dir),
        }
    }
    pub fn is_running(&self) -> bool {
        self.save_texture_task.lock().unwrap().is_running()
            || self.save_data_task.lock().unwrap().is_running()
    }
    pub fn count(&self) -> i32 {
        self.save_texture_task.lock().unwrap().count()
            + self.save_data_task.lock().unwrap().count()
            + self.load_all_packs_task.lock().unwrap().count()
    }

    pub fn save_texture(&self, texture_pack: &mut LoadedPackTexture, status: bool) {
        //saved on load, or change of list of what to display
        if status {
            std::mem::take(&mut texture_pack._is_dirty);
            let t = self.save_texture_task.lock().unwrap();
            let _ = t.send(texture_pack.clone());
            t.recv().unwrap().unwrap(); //expose errors of the save function call. If it had an error, we shall crash.
        }
    }

    pub fn save_data(&self, data_pack: &mut LoadedPackData, status: bool) {
        if status {
            std::mem::take(&mut data_pack._is_dirty);
            let _ = self.save_data_task.lock().unwrap().send(data_pack.clone());
        }
    }
    pub fn save_report(&self, target_dir: PathBuf, report: PackageImportReport, status: bool) {
        if status {
            let _ = self
                .save_report_task
                .lock()
                .unwrap()
                .send((target_dir, report));
        }
    }
    pub fn load_all_packs(&self, jokolay_dir: Arc<Dir>, root_path: std::path::PathBuf) {
        match self
            .load_all_packs_task
            .lock()
            .unwrap()
            .send((jokolay_dir, root_path))
        {
            Ok(_) => {}
            Err(e) => error!(?e),
        }
    }
    pub fn wait_for_load_all_packs(&self) -> Result<ImportAllTriplet, String> {
        self.load_all_packs_task.lock().unwrap().recv().unwrap()
    }

    #[allow(dead_code, unused)]
    fn change_map(
        &self,
        pack: &mut LoadedPackData,
        b2u_sender: &std::sync::mpsc::Sender<MessageToPackageUI>,
        link: &MumbleLink,
        currently_used_files: &BTreeMap<Uuid, bool>,
    ) {
        //TODO
        unimplemented!("PackTask::change_map is not implemented");
    }

    fn async_save_texture(pack_texture: LoadedPackTexture) -> Result<(), String> {
        trace!("Save texture package {:?}", pack_texture.path);

        match serde_json::to_string_pretty(&pack_texture.selectable_categories) {
            Ok(cs_json) => {
                let target = pack_texture
                    .path
                    .join(LoadedPackData::CATEGORY_SELECTION_FILE_NAME);
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(target)
                    .expect("failed to open category selection data file on disk")
                    .write_all(cs_json.as_bytes())
                    .expect("failed to write category selection data to disk");
            }
            Err(e) => {
                error!(?e, "failed to serialize cat selection");
            }
        }
        match serde_json::to_string_pretty(&pack_texture.activation_data) {
            Ok(ad_json) => {
                let target = pack_texture
                    .path
                    .join(LoadedPackTexture::ACTIVATION_DATA_FILE_NAME);
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(target)
                    .expect("failed to open activation data file on disk")
                    .write_all(ad_json.as_bytes())
                    .expect("failed to write activation data to disk");
            }
            Err(e) => {
                error!(?e, "failed to serialize activation");
            }
        }
        let target = pack_texture.path.join(LoadedPackData::CORE_PACK_DIR_NAME);
        save_pack_texture_to_dir(&pack_texture, &target)
    }

    fn async_save_data(pack_data: LoadedPackData) -> Result<(), String> {
        trace!("Save data package {:?}", pack_data.path);
        let target = pack_data.path.join(LoadedPackData::CORE_PACK_DIR_NAME);
        save_pack_data_to_dir(&pack_data, &target)?;
        Ok(())
    }

    fn async_save_report(input: (PathBuf, PackageImportReport)) -> Result<(), String> {
        let (writing_directory, report) = input;
        trace!("Save report package {:?}", writing_directory);
        match serde_json::to_string_pretty(&report) {
            Ok(cs_json) => {
                let target = writing_directory.join(PackageImportReport::REPORT_FILE_NAME);
                std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(target)
                    .expect("failed to open import quality report file on disk")
                    .write_all(cs_json.as_bytes())
                    .expect("failed to write import quality report to disk");
            }
            Err(e) => {
                error!(?e, "failed to serialize import quality report");
            }
        }
        Ok(())
    }
}

impl LoadedPackData {
    const CORE_PACK_DIR_NAME: &'static str = "core";
    const CATEGORY_SELECTION_FILE_NAME: &'static str = "cats.json";

    fn load_selectable_categories(
        path: &Path,
        pack: &PackCore,
    ) -> OrderedHashMap<String, CategorySelection> {
        //FIXME: we need to patch those categories from the one in the files
        let target = path.join(Self::CATEGORY_SELECTION_FILE_NAME);
        trace!("load_selectable_categories open {:?}", target);
        let mut cd_json = String::new();
        (if let Ok(mut file) = std::fs::OpenOptions::new().read(true).open(&target) {
            match file.read_to_string(&mut cd_json) {
                Ok(_n) => match serde_json::from_str(&cd_json) {
                    Ok(cd) => Some(cd),
                    Err(e) => {
                        error!(?e, "failed to deserialize category data");
                        None
                    }
                },
                Err(e) => {
                    error!(?e, "failed to read string of category data");
                    None
                }
            }
        } else {
            None
        })
        .flatten()
        .unwrap_or_else(|| {
            let cs = CategorySelection::default_from_pack_core(pack);
            match serde_json::to_string_pretty(&cs) {
                Ok(cs_json) => {
                    let target = path.join(Self::CATEGORY_SELECTION_FILE_NAME);
                    std::fs::OpenOptions::new()
                        .write(true)
                        .create(true)
                        .truncate(true)
                        .open(target)
                        .expect("failed to open category file on disk")
                        .write_all(cs_json.as_bytes())
                        .expect("failed to write category data to disk");
                }
                Err(e) => {
                    error!(?e, "failed to serialize cat selection");
                }
            }
            cs
        })
    }

    fn load_import_report(pack_dir: &Arc<Dir>) -> Option<PackageImportReport> {
        //FIXME: we need to patch those categories from the one in the files
        (if pack_dir.is_file(PackageImportReport::REPORT_FILE_NAME) {
            match pack_dir.read_to_string(PackageImportReport::REPORT_FILE_NAME) {
                Ok(cd_json) => match serde_json::from_str(&cd_json) {
                    Ok(cd) => Some(cd),
                    Err(e) => {
                        error!(?e, "failed to deserialize import report");
                        None
                    }
                },
                Err(e) => {
                    error!(?e, "failed to read string of import report");
                    None
                }
            }
        } else {
            None
        })
        .flatten()
    }
    pub fn load_from_dir(name: String, pack_dir: Arc<Dir>, path: PathBuf) -> Result<Self, String> {
        if !pack_dir
            .try_exists(Self::CORE_PACK_DIR_NAME)
            .or(Err("failed to check if pack core exists"))?
        {
            return Err("pack core doesn't exist in this pack".to_string());
        }
        let core_dir = pack_dir
            .open_dir(Self::CORE_PACK_DIR_NAME)
            .or(Err("failed to open core pack directory"))?;
        let start = std::time::SystemTime::now();
        let import_report = LoadedPackData::load_import_report(&pack_dir);
        let core = load_pack_core_from_normalized_folder(&core_dir, import_report)
            .or(Err("failed to load pack from dir"))?;
        let elaspsed = start.elapsed().unwrap_or_default();
        tracing::info!(
            "Loading of package from disk {} took {} ms",
            name,
            elaspsed.as_millis()
        );

        //FIXME: Since categories have randomly generated uuids (and not saved), one need to build from those, all the time.
        //let selectable_categories = CategorySelection::default_from_pack_core(&core);
        let selectable_categories = Self::load_selectable_categories(&path, &core);

        Ok(LoadedPackData {
            name,
            uuid: core.uuid,
            path,
            selected_files: Default::default(),
            all_categories: core.all_categories,
            categories: core.categories,
            maps: core.maps,
            source_files: core.active_source_files,
            _is_dirty: false,
            active_elements: Default::default(),
            activation_data: Default::default(),
            selectable_categories,
            entities_parents: core.entities_parents,
        })
    }

    pub fn category_set(&mut self, uuid: Uuid, status: bool) -> bool {
        if CategorySelection::recursive_set(&mut self.selectable_categories, uuid, status) {
            self._is_dirty = true;
            true
        } else {
            false
        }
    }
    pub fn category_branch_set(&mut self, uuid: Uuid, status: bool) -> bool {
        if let Some(cs) = CategorySelection::get(&mut self.selectable_categories, uuid) {
            cs.is_selected = status;
            self._is_dirty = true;
            if CategorySelection::recursive_set(&mut cs.children, uuid, status) {
                return true;
            }
        }
        false
    }
    pub fn category_set_all(&mut self, status: bool) {
        CategorySelection::recursive_set_all(&mut self.selectable_categories, status);
        self._is_dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self._is_dirty
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tick(
        &mut self,
        b2u_sender: &tokio::sync::mpsc::Sender<ComponentMessage>,
        link: &MumbleLink,
        currently_used_files: &BTreeMap<Uuid, bool>,
        list_of_active_or_selected_elements_changed: bool,
        map_changed: bool,
        _tasks: &PackTasks,
        next_loaded: &mut HashSet<Uuid>,
    ) {
        //since the loading of texture is lazy, there is no problem when calling this regularly
        if map_changed || list_of_active_or_selected_elements_changed {
            //tasks.change_map(self, b2u_sender, link, currently_used_files);
            let mut active_elements: HashSet<Uuid> = Default::default();
            self.on_map_changed(b2u_sender, link, currently_used_files, &mut active_elements);
            let _ = b2u_sender.blocking_send(to_data(MessageToPackageUI::PackageActiveElements(
                self.uuid,
                active_elements.clone(),
            )));
            self.active_elements = active_elements.clone();
            next_loaded.extend(active_elements);
        }
    }

    fn on_map_changed(
        &mut self,
        b2u_sender: &tokio::sync::mpsc::Sender<ComponentMessage>,
        link: &MumbleLink,
        currently_used_files: &BTreeMap<Uuid, bool>,
        active_elements: &mut HashSet<Uuid>,
    ) {
        info!(link.map_id, "current map data is updated. {}", self.name);
        if link.map_id == 0 {
            info!("No map do not do anything");
            return;
        }
        debug!(
            "Start building SelectedCategoryManager {}",
            self.selectable_categories.len()
        );
        let selected_categories_manager =
            SelectedCategoryManager::new(&self.selectable_categories, &self.categories);

        debug!("Start building SelectedFileManager");
        let selected_files_manager = SelectedFileManager::new(
            &self.selected_files,
            &self.source_files,
            currently_used_files,
        );

        debug!("Start loading markers");
        let mut nb_markers_attempt = 0;
        let mut nb_markers_loaded = 0;
        for marker in self
            .maps
            .get(&link.map_id)
            .unwrap_or(&Default::default())
            .markers
            .values()
        {
            nb_markers_attempt += 1;
            if selected_files_manager.is_selected(&marker.source_file_uuid) {
                active_elements.insert(marker.guid);
                active_elements.insert(marker.parent);
                if selected_categories_manager.is_selected(&marker.parent) {
                    let category_attributes = selected_categories_manager.get(&marker.parent);
                    let mut common_attributes = marker.attrs.clone(); // why a clone ?
                    common_attributes.inherit_if_attr_none(category_attributes);
                    let key = &marker.guid;
                    if let Some(behavior) = common_attributes.get_behavior() {
                        if match behavior {
                            Behavior::AlwaysVisible => false,
                            Behavior::ReappearOnMapChange
                            | Behavior::ReappearOnDailyReset
                            | Behavior::OnlyVisibleBeforeActivation
                            | Behavior::ReappearAfterTimer
                            | Behavior::ReappearOnMapReset
                            | Behavior::WeeklyReset => {
                                self.activation_data.global.contains_key(key)
                            }
                            Behavior::OncePerInstance => self
                                .activation_data
                                .global
                                .get(key)
                                .map(|a| match a {
                                    ActivationType::Instance(a) => a == &link.server_address,
                                    _ => false,
                                })
                                .unwrap_or_default(),
                            Behavior::DailyPerChar => self
                                .activation_data
                                .character
                                .get(&link.name)
                                .map(|a| a.contains_key(key))
                                .unwrap_or_default(),
                            Behavior::OncePerInstancePerChar => self
                                .activation_data
                                .character
                                .get(&link.name)
                                .map(|a| {
                                    a.get(key)
                                        .map(|a| match a {
                                            ActivationType::Instance(a) => {
                                                a == &link.server_address
                                            }
                                            _ => false,
                                        })
                                        .unwrap_or_default()
                                })
                                .unwrap_or_default(),
                            Behavior::WvWObjective => {
                                false // ???
                            }
                        } {
                            continue;
                        }
                    }
                    if let Some(tex_path) = common_attributes.get_icon_file() {
                        let _ =
                            b2u_sender.blocking_send(to_data(MessageToPackageUI::MarkerTexture(
                                self.uuid,
                                tex_path.clone(),
                                marker.guid,
                                marker.position,
                                common_attributes,
                            )));
                    } else {
                        debug!("no texture attribute on this marker");
                    }

                    nb_markers_loaded += 1;
                } else {
                    debug!(
                        "category {} = {} is not enabled",
                        marker.category, marker.parent
                    );
                }
            }
        }

        debug!("Start loading trails");
        let mut nb_trails_attempt = 0;
        let mut nb_trails_loaded = 0;
        for trail in self
            .maps
            .get(&link.map_id)
            .unwrap_or(&Default::default())
            .trails
            .values()
        {
            nb_trails_attempt += 1;
            if selected_files_manager.is_selected(&trail.source_file_uuid) {
                active_elements.insert(trail.guid);
                active_elements.insert(trail.parent);
                if selected_categories_manager.is_selected(&trail.parent) {
                    let category_attributes = selected_categories_manager.get(&trail.parent);
                    let mut common_attributes = trail.props.clone();
                    common_attributes.inherit_if_attr_none(category_attributes);
                    if let Some(tex_path) = common_attributes.get_texture() {
                        let _ =
                            b2u_sender.blocking_send(to_data(MessageToPackageUI::TrailTexture(
                                self.uuid,
                                tex_path.clone(),
                                trail.guid,
                                common_attributes,
                            )));
                    } else {
                        debug!("no texture attribute on this trail");
                    }
                    nb_trails_loaded += 1;
                } else {
                    debug!(
                        "category {} = {} is not enabled",
                        trail.category, trail.parent
                    );
                }
            }
        }
        info!(
            "Load notifications for {} on map {}: {}/{} markers and {}/{} trails",
            self.name,
            link.map_id,
            nb_markers_loaded,
            nb_markers_attempt,
            nb_trails_loaded,
            nb_trails_attempt
        );
        debug!(
            "active categories: {:?}",
            selected_categories_manager.keys()
        );
    }
}

impl LoadedPackTexture {
    const ACTIVATION_DATA_FILE_NAME: &'static str = "activation.json";

    pub fn category_set_all(&mut self, status: bool) {
        CategorySelection::recursive_set_all(&mut self.selectable_categories, status);
        self._is_dirty = true;
    }

    pub fn update_active_categories(&mut self, active_elements: &HashSet<Uuid>) {
        CategorySelection::recursive_update_active_categories(
            &mut self.selectable_categories,
            active_elements,
        );
    }
    pub fn category_sub_menu(
        &mut self,
        back_end_notifier: &tokio::sync::mpsc::Sender<ComponentMessage>,
        ui: &mut egui::Ui,
        show_only_active: bool,
        import_quality_report: &PackageImportReport,
    ) {
        //it is important to generate a new id each time to avoid collision
        ui.push_id(ui.next_auto_id(), |ui| {
            CategorySelection::recursive_selection_ui(
                back_end_notifier,
                &mut self.selectable_categories,
                ui,
                &mut self._is_dirty,
                show_only_active,
                import_quality_report,
            );
        });
        if self._is_dirty {
            let _ = back_end_notifier.blocking_send(to_data(
                MessageToPackageBack::CategoryActivationStatusChanged,
            ));
        }
    }

    pub fn is_dirty(&self) -> bool {
        self._is_dirty
    }
    pub(crate) fn tick(
        &mut self,
        renderer_notifier: &tokio::sync::mpsc::Sender<ComponentMessage>,
        _timestamp: f64,
        link: &MumbleLink,
        //next_on_screen: &mut HashSet<Uuid>,
        z_near: f32,
        _tasks: &PackTasks,
    ) -> Result<()> {
        tracing::trace!(
            "LoadedPackTexture.tick: {} {}-{} {}-{}",
            self.name,
            self.current_map_data.active_markers.len(),
            self.current_map_data.wip_markers.len(),
            self.current_map_data.active_trails.len(),
            self.current_map_data.wip_trails.len(),
        );
        let mut marker_objects = Vec::new();
        for marker in self.current_map_data.active_markers.values() {
            if let Some(mo) = marker.get_vertices_and_texture(link, z_near) {
                marker_objects.push(mo);
            }
        }
        tracing::trace!(
            "LoadedPackTexture.tick: {}, markers {}",
            self.name,
            marker_objects.len()
        );
        let _ = renderer_notifier
            .blocking_send(to_data(MessageToRenderer::BulkMarkerObject(marker_objects)));
        let mut trail_objects = Vec::new();
        for trail in self.current_map_data.active_trails.values() {
            trail_objects.push(TrailObject {
                vertices: trail.trail_object.vertices.clone(),
                texture: trail.trail_object.texture,
            });
            //next_on_screen.insert(*uuid);
        }
        tracing::trace!(
            "LoadedPackTexture.tick: {}, trails {}",
            self.name,
            trail_objects.len()
        );
        let _ = renderer_notifier
            .blocking_send(to_data(MessageToRenderer::BulkTrailObject(trail_objects)));
        Ok(())
    }

    pub fn swap(&mut self) {
        info!(
            "swap {} to display {} textures, {} markers, {} trails",
            self.name,
            self.current_map_data.active_textures.len(),
            self.current_map_data.wip_markers.len(),
            self.current_map_data.wip_trails.len()
        );
        self.current_map_data.active_markers =
            std::mem::take(&mut self.current_map_data.wip_markers);
        self.current_map_data.active_trails = std::mem::take(&mut self.current_map_data.wip_trails);
    }

    pub fn load_marker_texture(
        &mut self,
        egui_context: &egui::Context,
        default_tex_id: &TextureHandle,
        tex_path: &RelativePath,
        marker_uuid: Uuid,
        position: Vec3,
        common_attributes: CommonAttributes,
    ) {
        if !self.current_map_data.active_textures.contains_key(tex_path) {
            if let Some(tex) = self.textures.get(tex_path) {
                let img = image::load_from_memory(tex).unwrap();

                //TODO: insertion must happen inside the UI => egui_context should never be transmitted on a tick()
                self.current_map_data.active_textures.insert(
                    tex_path.clone(),
                    egui_context.load_texture(
                        tex_path.as_str(),
                        ColorImage::from_rgba_unmultiplied(
                            [img.width() as _, img.height() as _],
                            img.into_rgba8().as_bytes(),
                        ),
                        Default::default(),
                    ),
                );
            } else {
                error!(%tex_path, "failed to find this icon texture");
            }
        }
        let th = self
            .current_map_data
            .active_textures
            .get(tex_path)
            .unwrap_or(default_tex_id);
        let texture_id = match th.id() {
            egui::TextureId::Managed(i) => i,
            egui::TextureId::User(_) => todo!(),
        };

        let max_pixel_size = common_attributes.get_max_size().copied().unwrap_or(2048.0); // default taco max size
        let min_pixel_size = common_attributes.get_min_size().copied().unwrap_or(5.0); // default taco min size
        let am = ActiveMarker {
            texture_id,
            _texture: th.clone(),
            common_attributes,
            pos: position,
            max_pixel_size,
            min_pixel_size,
        };
        self.current_map_data.wip_markers.insert(marker_uuid, am);
    }

    pub fn load_trail_texture(
        &mut self,
        egui_context: &egui::Context,
        default_tex_id: &TextureHandle,
        tex_path: &RelativePath,
        trail_uuid: Uuid,
        common_attributes: CommonAttributes,
    ) {
        if !self.current_map_data.active_textures.contains_key(tex_path) {
            if let Some(tex) = self.textures.get(tex_path) {
                let img = image::load_from_memory(tex).unwrap();
                self.current_map_data.active_textures.insert(
                    tex_path.clone(),
                    egui_context.load_texture(
                        tex_path.as_str(),
                        ColorImage::from_rgba_unmultiplied(
                            [img.width() as _, img.height() as _],
                            img.into_rgba8().as_bytes(),
                        ),
                        Default::default(),
                    ),
                );
            } else {
                error!(%tex_path, "failed to find this trail texture");
            }
        } else {
            trace!("Trail texture already loaded {:?}", tex_path);
        }
        let texture_path = common_attributes.get_texture();
        let th = texture_path
            .and_then(|path| self.current_map_data.active_textures.get(path))
            .unwrap_or(default_tex_id);

        let tbin_path = if let Some(tbin) = common_attributes.get_trail_data() {
            debug!(?texture_path, "tbin path");
            tbin
        } else {
            info!(?trail_uuid, "missing tbin path");
            return;
        };
        let tbin = if let Some(tbin) = self.tbins.get(tbin_path) {
            tbin
        } else {
            info!(%tbin_path, "failed to find tbin");
            return;
        };
        if let Some(active_trail) =
            ActiveTrail::get_vertices_and_texture(&common_attributes, &tbin.nodes, th.clone())
        {
            self.current_map_data
                .wip_trails
                .insert(trail_uuid, active_trail);
        } else {
            info!("Cannot display {texture_path:?}")
        }
    }
}

pub fn jokolay_to_editable_path(jokolay_path: &std::path::Path) -> std::path::PathBuf {
    jokolay_path
        .join(PACKAGE_MANAGER_DIRECTORY_NAME)
        .join(EDITABLE_PACKAGE_NAME)
}

pub fn jokolay_to_extract_path(jokolay_path: &std::path::Path) -> std::path::PathBuf {
    jokolay_path
        .join(PACKAGE_MANAGER_DIRECTORY_NAME)
        .join(EXTRACT_DIRECTORY_NAME)
}

pub fn jokolay_to_marker_path(jokolay_path: &std::path::Path) -> std::path::PathBuf {
    jokolay_path
        .join(PACKAGE_MANAGER_DIRECTORY_NAME)
        .join(PACKAGES_DIRECTORY_NAME)
}

pub fn jokolay_to_marker_dir(jokolay_dir: &Arc<Dir>) -> Result<Dir> {
    jokolay_dir
        .create_dir_all(PACKAGE_MANAGER_DIRECTORY_NAME)
        .into_diagnostic()
        .wrap_err(format!(
            "failed to create marker manager directory {}",
            PACKAGE_MANAGER_DIRECTORY_NAME
        ))?;
    let marker_manager_dir = jokolay_dir
        .open_dir(PACKAGE_MANAGER_DIRECTORY_NAME)
        .into_diagnostic()
        .wrap_err(format!(
            "failed to open marker manager directory {}",
            PACKAGE_MANAGER_DIRECTORY_NAME
        ))?;

    marker_manager_dir
        .create_dir_all(PACKAGES_DIRECTORY_NAME)
        .into_diagnostic()
        .wrap_err(format!(
            "failed to create marker packs directory {}",
            PACKAGES_DIRECTORY_NAME
        ))?;
    let marker_packs_dir = marker_manager_dir
        .open_dir(PACKAGES_DIRECTORY_NAME)
        .into_diagnostic()
        .wrap_err(format!(
            "failed to open marker packs dir {}",
            PACKAGES_DIRECTORY_NAME
        ))?;

    marker_manager_dir
        .create_dir_all(EDITABLE_PACKAGE_NAME)
        .into_diagnostic()
        .wrap_err("failed to create editable package directory")?;
    let editable_package = marker_manager_dir
        .open_dir(EDITABLE_PACKAGE_NAME)
        .into_diagnostic()
        .wrap_err("failed to open editable package directory")?;

    editable_package
        .create_dir_all("data")
        .into_diagnostic()
        .wrap_err("failed to create data folder for editable package")?;

    Ok(marker_packs_dir)
}

pub fn load_all_from_dir(
    input: (Arc<Dir>, std::path::PathBuf),
) -> Result<ImportAllTriplet, String> {
    let (jokolay_dir, root_path) = input;
    trace!("load_all_from_dir {:?}", root_path);
    let marker_packs_dir = match jokolay_to_marker_dir(&jokolay_dir) {
        Ok(marker_packs_dir) => marker_packs_dir,
        Err(e) => {
            error!("Failed to open packages directory {:?}", e);
            return Err("Failed to open packages directory".to_string());
        }
    };
    let marker_packs_path = jokolay_to_marker_path(&root_path);
    let mut data_packs: BTreeMap<Uuid, LoadedPackData> = Default::default();
    let mut texture_packs: BTreeMap<Uuid, LoadedPackTexture> = Default::default();
    let mut report_packs: BTreeMap<Uuid, PackageImportReport> = Default::default();

    for entry in marker_packs_dir
        .entries()
        .or(Err("failed to get entries of marker packs dir"))?
    {
        let entry = entry.or(Err("Failed to read packages directory"))?;
        if entry
            .metadata()
            .or(Err("Could not read folder metadata"))?
            .is_file()
        {
            continue;
        }
        if let Ok(name) = entry.file_name() {
            let pack_path = marker_packs_path.join(&name);
            let pack_dir = entry.open_dir().or(Err(format!(
                "failed to open pack entry as directory: {}",
                name
            )))?;
            {
                if name == EDITABLE_PACKAGE_NAME {
                    //TODO: have a version of loading that does not involve already ingested packages
                    if let Ok(pack_core) = load_pack_core_from_normalized_folder(&pack_dir, None) {
                        let lp = build_from_core(name.clone(), pack_path, pack_core);
                        let (data, tex, report) = lp;
                        data_packs.insert(data.uuid, data);
                        texture_packs.insert(tex.uuid, tex);
                        report_packs.insert(report.uuid, report);
                    }
                } else if name == LOCAL_EXPANDED_PACKAGE_NAME {
                    //ignore this package, it'll be overwriten
                } else {
                    let span_guard = info_span!("loading pack from dir", name).entered();

                    match build_from_dir(name.clone(), pack_dir.into(), pack_path) {
                        Ok(lp) => {
                            let (data, tex, report) = lp;
                            data_packs.insert(data.uuid, data);
                            texture_packs.insert(tex.uuid, tex);
                            report_packs.insert(report.uuid, report);
                        }
                        Err(e) => {
                            error!(?e, "failed to load pack from directory: {}", name);
                        }
                    }
                    drop(span_guard);
                }
            }
        }
    }
    Ok((data_packs, texture_packs, report_packs))
}

fn build_from_dir(
    name: String,
    pack_dir: Arc<Dir>,
    pack_path: PathBuf,
) -> Result<ImportTriplet, String> {
    if !pack_dir
        .try_exists(LoadedPackData::CORE_PACK_DIR_NAME)
        .or(Err("failed to check if pack core exists"))?
    {
        return Err("pack core doesn't exist in this pack".to_string());
    }
    let core_dir = pack_dir
        .open_dir(LoadedPackData::CORE_PACK_DIR_NAME)
        .or(Err("failed to open core pack directory"))?;
    let start = std::time::SystemTime::now();
    let import_report = LoadedPackData::load_import_report(&pack_dir);
    let core = load_pack_core_from_normalized_folder(&core_dir, import_report)
        .or(Err("failed to load pack from dir"))?;
    let elaspsed = start.elapsed().unwrap_or_default();
    tracing::info!(
        "Loading of package from disk {} took {} ms",
        name,
        elaspsed.as_millis()
    );
    let res = build_from_core(name.clone(), pack_path, core);
    Ok(res)
}

pub fn build_from_core(name: String, path: PathBuf, core: PackCore) -> ImportTriplet {
    let selectable_categories = LoadedPackData::load_selectable_categories(&path, &core);
    let data = LoadedPackData {
        name: name.clone(),
        uuid: core.uuid,
        path: path.clone(),
        selected_files: Default::default(),
        all_categories: core.all_categories,
        categories: core.categories,
        maps: core.maps,
        source_files: core.active_source_files.clone(),
        _is_dirty: false,
        activation_data: Default::default(),
        active_elements: Default::default(),
        selectable_categories: selectable_categories.clone(),
        entities_parents: core.entities_parents,
    };
    let target = path.join(LoadedPackTexture::ACTIVATION_DATA_FILE_NAME);
    let mut cd_json = String::new();
    let activation_data =
        (if let Ok(mut file) = std::fs::OpenOptions::new().read(true).open(target) {
            match file.read_to_string(&mut cd_json) {
                Ok(_n) => match serde_json::from_str(&cd_json) {
                    Ok(cd) => Some(cd),
                    Err(e) => {
                        error!(?e, "failed to deserialize activation data");
                        None
                    }
                },
                Err(e) => {
                    error!(?e, "failed to read string of category data");
                    None
                }
            }
        } else {
            None
        })
        .flatten()
        .unwrap_or_default();
    let tex = LoadedPackTexture {
        uuid: core.uuid,
        selectable_categories,
        textures: core.textures,
        current_map_data: Default::default(),
        _is_dirty: false,
        activation_data,
        path,
        name,
        tbins: core.tbins,
        //active_elements: Default::default(),
        source_files: core.active_source_files,
    };
    let report = core.report;
    (data, tex, report)
}
