use std::{
    collections::{BTreeMap, HashMap, HashSet}, sync::Arc
};

use indexmap::IndexMap;
use ordered_hash_map::OrderedHashMap;

use cap_std::fs_utf8::Dir;
use egui::{ColorImage, TextureHandle};
use image::EncodableLayout;
use tracing::{debug, error, info, info_span};
use uuid::Uuid;

use crate::{
    io::{load_pack_core_from_dir, save_pack_data_to_dir, save_pack_texture_to_dir}, manager::pack::{category_selection::SelectedCategoryManager, file_selection::SelectedFileManager}, message::{UIToBackMessage, UIToUIMessage}, pack::{Category, CommonAttributes, MapData, PackCore, TBin}
};
use jokolink::MumbleLink;
use joko_core::{
    task::{AsyncTask, AsyncTaskGuard},
    RelativePath
};
use crate::message::{
    BackToUIMessage, TrailObject
};
use miette::{bail, Context, IntoDiagnostic, Result};

use super::activation::{ActivationData, ActivationType};
use super::active::{CurrentMapData, ActiveMarker, ActiveTrail};
use crate::manager::pack::category_selection::CategorySelection;
use crate::manager::package::{PACKAGES_DIRECTORY_NAME, PACKAGE_MANAGER_DIRECTORY_NAME};


//TODO: separate in front and back tasks
pub (crate) struct PackTasks {
    //an object that can handle such tasks should be passed as argument of any function that may required an async action
    save_texture_task: AsyncTask<LoadedPackTexture, Result<()>>,
    save_data_task: AsyncTask<LoadedPackData, Result<()>>,
    load_all_packs_task: AsyncTask<Arc<Dir>, Result<(BTreeMap<Uuid, LoadedPackData>, BTreeMap<Uuid, LoadedPackTexture>)>>
}

#[derive(Clone)]
pub struct LoadedPackData {
    pub name: String,
    pub uuid: Uuid,
    pub dir: Arc<Dir>,
    /// The actual xml pack.
    //pub core: PackCore,
    pub categories: IndexMap<Uuid, Category>,
    pub all_categories: HashMap<String, Uuid>,
    pub source_files: BTreeMap<String, bool>,//TODO: have a reference containing pack name and maybe even path inside the package
    pub maps: HashMap<u32, MapData>,
    selected_files: BTreeMap<String, bool>,
    _is_dirty: bool,//there was an edition in the package itself

    // loca copy in the data side of what is exposed in UI
    selectable_categories: OrderedHashMap<String, CategorySelection>,
    pub entities_parents: HashMap<Uuid, Uuid>,
    activation_data: ActivationData,
    active_elements: HashSet<Uuid>,//keep track of which elements are active
}

#[derive(Clone)]
pub struct LoadedPackTexture {
    pub name: String,
    pub uuid: Uuid,
    /// The directory inside which the pack data is stored
    /// There should be a subdirectory called `core` which stores the pack core
    /// Files related to Jokolay thought will have to be stored directly inside this directory, to keep the xml subdirectory clean.
    /// eg: Active categories, activation data etc..
    pub dir: Arc<Dir>,
    pub tbins: HashMap<RelativePath, TBin>,
    pub textures: HashMap<RelativePath, Vec<u8>>,

    /// The selection of categories which are "enabled" and markers belonging to these may be rendered
    selectable_categories: OrderedHashMap<String, CategorySelection>,
    current_map_data: CurrentMapData,
    activation_data: ActivationData,
    active_elements: HashSet<Uuid>,//which are the active elements (loaded)
    pub late_discovery_categories: HashSet<Uuid>,//categories that are defined only from a marker point of view. It needs to be saved in some way or it's lost at next start.
    _is_dirty: bool,
}

impl PackTasks {
    pub fn new() -> Self {
        Self {
            save_texture_task: AsyncTaskGuard::new(PackTasks::async_save_texture),
            save_data_task: AsyncTaskGuard::new(PackTasks::async_save_data),
            load_all_packs_task: AsyncTaskGuard::new(load_all_from_dir),
        }
    }
    pub fn is_running(&self) -> bool {
        self.save_texture_task.lock().unwrap().is_running() ||
        self.save_data_task.lock().unwrap().is_running()
    }
    pub fn count(&self) -> i32 {
        0
        + self.save_texture_task.lock().unwrap().count()
        + self.save_data_task.lock().unwrap().count()
        + self.load_all_packs_task.lock().unwrap().count()
    }
    
    pub fn save_texture(&self, texture_pack: &mut LoadedPackTexture, status: bool) {
        if status {
            std::mem::take(&mut texture_pack._is_dirty);
            self.save_texture_task.lock().unwrap().send(
                texture_pack.clone()
            );
        }
    }

    pub fn save_data(&self, data_pack: &mut LoadedPackData, status: bool) {
        if status {
            std::mem::take(&mut data_pack._is_dirty);
            self.save_data_task.lock().unwrap().send(
                data_pack.clone()
            );
        }
    }
    pub fn load_all_packs(&self, jokolay_dir: Arc<Dir>) {
        self.load_all_packs_task.lock().unwrap().send(
            jokolay_dir
        );
    }
    pub fn wait_for_load_all_packs(&self) -> Result<(BTreeMap<Uuid, LoadedPackData>, BTreeMap<Uuid, LoadedPackTexture>)> {
        self.load_all_packs_task.lock().unwrap().recv().unwrap()
    }

    fn change_map(
        &self, 
        pack: &mut LoadedPackData,
        b2u_sender: &std::sync::mpsc::Sender<BackToUIMessage>,
        link: &MumbleLink,
        currently_used_files: &BTreeMap<String, bool>
    ) {
        //TODO
        //self.load_map_task.lock().unwrap().send(pack);
    }

    fn async_save_texture(
        pack_texture: LoadedPackTexture
    ) -> Result<()> {
        info!("Save texture package {:?}", pack_texture.dir);
        match serde_json::to_string_pretty(&pack_texture.selectable_categories) {
            Ok(cs_json) => match pack_texture.dir.write(LoadedPackData::CATEGORY_SELECTION_FILE_NAME, cs_json) {
                Ok(_) => {
                    debug!("wrote cat selections to disk after creating a default from pack");
                }
                Err(e) => {
                    debug!(?e, "failed to write category data to disk");
                }
            },
            Err(e) => {
                error!(?e, "failed to serialize cat selection");
            }
        }
        match serde_json::to_string_pretty(&pack_texture.activation_data) {
            Ok(ad_json) => match pack_texture.dir.write(LoadedPackTexture::ACTIVATION_DATA_FILE_NAME, ad_json) {
                Ok(_) => {
                    debug!("wrote activation to disk after creating a default from pack");
                }
                Err(e) => {
                    debug!(?e, "failed to write activation data to disk");
                }
            },
            Err(e) => {
                error!(?e, "failed to serialize activation");
            }
        }
        let writing_directory = pack_texture.dir
            .open_dir(LoadedPackData::CORE_PACK_DIR_NAME)
            .into_diagnostic()
            .wrap_err("failed to open core pack directory")?;
        save_pack_texture_to_dir(&pack_texture, &writing_directory)?;
        Ok(())
    }

    fn async_save_data(
        pack_data: LoadedPackData
    ) -> Result<()> {
        info!("Save data package {:?}", pack_data.dir);
        pack_data.dir
            .create_dir_all(LoadedPackData::CORE_PACK_DIR_NAME)
            .into_diagnostic()
            .wrap_err("failed to create xmlpack directory")?;
        let writing_directory = pack_data.dir
            .open_dir(LoadedPackData::CORE_PACK_DIR_NAME)
            .into_diagnostic()
            .wrap_err("failed to open core pack directory")?;
        save_pack_data_to_dir(
            &pack_data,
            &writing_directory,
        )?;
        Ok(())
    }

}


impl LoadedPackData {
    const CORE_PACK_DIR_NAME: &'static str = "core";
    const CATEGORY_SELECTION_FILE_NAME: &'static str = "cats.json";

    fn load_selectable_categories(pack_dir: &Arc<Dir>, pack: &PackCore) -> OrderedHashMap<String, CategorySelection> {
        //FIXME: we need to patch those categories from the one in the files
        (if pack_dir.is_file(Self::CATEGORY_SELECTION_FILE_NAME) {
            match pack_dir.read_to_string(Self::CATEGORY_SELECTION_FILE_NAME) {
                Ok(cd_json) => match serde_json::from_str(&cd_json) {
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
            let cs = CategorySelection::default_from_pack_core(&pack);
            match serde_json::to_string_pretty(&cs) {
                Ok(cs_json) => match pack_dir.write(Self::CATEGORY_SELECTION_FILE_NAME, cs_json) {
                    Ok(_) => {
                        debug!("wrote cat selections to disk after creating a default from pack");
                    }
                    Err(e) => {
                        debug!(?e, "failed to write category data to disk");
                    }
                },
                Err(e) => {
                    error!(?e, "failed to serialize cat selection");
                }
            }
            cs
        })
    }
    pub fn load_from_dir(name: String, pack_dir: Arc<Dir>) -> Result<Self> {
        if !pack_dir
            .try_exists(Self::CORE_PACK_DIR_NAME)
            .into_diagnostic()
            .wrap_err("failed to check if pack core exists")?
        {
            bail!("pack core doesn't exist in this pack");
        }
        let core_dir = pack_dir
            .open_dir(Self::CORE_PACK_DIR_NAME)
            .into_diagnostic()
            .wrap_err("failed to open core pack directory")?;
        let start = std::time::SystemTime::now();
        let core = load_pack_core_from_dir(&core_dir).wrap_err("failed to load pack from dir")?;
        let elaspsed = start.elapsed().unwrap_or_default();
        tracing::info!("Loading of package from disk {} took {} ms", name, elaspsed.as_millis());
    
        //FIXME: Since categories have randomly generated uuids (and not saved), one need to build from those, all the time.
        //let selectable_categories = CategorySelection::default_from_pack_core(&core);
        let selectable_categories = Self::load_selectable_categories(&pack_dir, &core);
        
        Ok(LoadedPackData {
            name,
            uuid: core.uuid,
            dir: pack_dir,
            selected_files: Default::default(),
            all_categories: core.all_categories,
            categories: core.categories,
            maps: core.maps,
            source_files: core.source_files,
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
    pub fn category_set_all(&mut self, status: bool) {
        CategorySelection::recursive_set_all(&mut self.selectable_categories, status);
        self._is_dirty = true;
    }

    pub fn is_dirty(&self) -> bool {
        self._is_dirty
    }

    pub fn tick(
        &mut self,
        b2u_sender: &std::sync::mpsc::Sender<BackToUIMessage>,
        loop_index: u128,
        link: &MumbleLink,
        currently_used_files: &BTreeMap<String, bool>,
        list_of_active_or_selected_elements_changed: bool,
        map_changed: bool,
        tasks: &PackTasks,
        next_loaded: &mut HashSet<Uuid>,
    ) {
        //since the loading of texture is lazy, there is no problem when calling this regularly
        if map_changed || list_of_active_or_selected_elements_changed {
            tasks.change_map(self, b2u_sender, link, currently_used_files);
            let mut active_elements: HashSet<Uuid> = Default::default();
            self.on_map_changed(b2u_sender, link, currently_used_files, &mut active_elements);
            b2u_sender.send(BackToUIMessage::PackageActiveElements(self.uuid, active_elements.clone()));
            self.active_elements = active_elements.clone();
            next_loaded.extend(active_elements);
        }
    }
    
    fn on_map_changed(
        &mut self,
        b2u_sender: &std::sync::mpsc::Sender<BackToUIMessage>,
        link: &MumbleLink,
        currently_used_files: &BTreeMap<String, bool>,
        active_elements: &mut HashSet<Uuid>,
    ){
        info!(link.map_id, "current map data is updated. {}", self.name);
        if link.map_id == 0 {
            info!("No map do not do anything");
            return;
        }
        debug!("Start building SelectedCategoryManager {}", self.selectable_categories.len());
        let selected_categories_manager = SelectedCategoryManager::new(&self.selectable_categories, &self.categories);

        debug!("Start building SelectedFileManager");
        let selected_files_manager = SelectedFileManager::new(&self.selected_files, &self.source_files, &currently_used_files);
        
        debug!("Start loading markers");
        let mut nb_markers_attempt = 0;
        let mut nb_markers_loaded = 0;
        for (_index, marker) in self
            .maps
            .get(&link.map_id)
            .unwrap_or(&Default::default())
            .markers
            .values()
            .enumerate()
        {
            nb_markers_attempt += 1;
            if selected_files_manager.is_selected(&marker.source_file_name) {
                active_elements.insert(marker.guid);
                active_elements.insert(marker.parent);
                if selected_categories_manager.is_selected(&marker.parent) {
                    let category_attributes = selected_categories_manager.get(&marker.parent);
                    let mut common_attributes = marker.attrs.clone();// why a clone ?
                    common_attributes.inherit_if_attr_none(category_attributes);
                    let key = &marker.guid;
                    if let Some(behavior) = common_attributes.get_behavior() {
                        use crate::pack::Behavior;
                        if match behavior {
                            Behavior::AlwaysVisible => false,
                            Behavior::ReappearOnMapChange
                            | Behavior::ReappearOnDailyReset
                            | Behavior::OnlyVisibleBeforeActivation
                            | Behavior::ReappearAfterTimer
                            | Behavior::ReappearOnMapReset
                            | Behavior::WeeklyReset => self.activation_data.global.contains_key(key),
                            Behavior::OncePerInstance => self
                                .activation_data
                                .global
                                .get(key)
                                .map(|a| match a {
                                    ActivationType::Instance(a) => a == &link.server_address,
                                    _ => false,
                                })
                                .unwrap_or_default(),
                            Behavior::DailyPerChar => 
                            self.activation_data
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
                                            ActivationType::Instance(a) => a == &link.server_address,
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
                        b2u_sender.send(BackToUIMessage::MarkerTexture(self.uuid, tex_path.clone(), marker.guid, marker.position, common_attributes));
                    } else {
                        debug!("no texture attribute on this marker");
                    }
                    
                    nb_markers_loaded += 1;
                } else {
                    debug!("category {} = {} is not enabled", marker.category, marker.parent);
                }
            }
        }

        debug!("Start loading trails");
        let mut nb_trails_attempt = 0;
        let mut nb_trails_loaded = 0;
        for (_index, trail) in self
            .maps
            .get(&link.map_id)
            .unwrap_or(&Default::default())
            .trails
            .values()
            .enumerate()
        {
            nb_trails_attempt += 1;
            if selected_files_manager.is_selected(&trail.source_file_name) {
                active_elements.insert(trail.guid);
                active_elements.insert(trail.parent);
                if selected_categories_manager.is_selected(&trail.parent) {
                        let category_attributes = selected_categories_manager.get(&trail.parent);
                    let mut common_attributes = trail.props.clone();
                    common_attributes.inherit_if_attr_none(category_attributes);
                    if let Some(tex_path) = common_attributes.get_texture() {
                        b2u_sender.send(BackToUIMessage::TrailTexture(self.uuid, tex_path.clone(), trail.guid, common_attributes));
                    } else {
                        debug!("no texture attribute on this trail");
                    }
                    nb_trails_loaded += 1;
                } else {
                    debug!("category {} = {} is not enabled", trail.category, trail.parent);
                }
            }
        }
        info!("Load notifications for {} on map {}: {}/{} markers and {}/{} trails", self.name, link.map_id, nb_markers_loaded, nb_markers_attempt, nb_trails_loaded, nb_trails_attempt);
        debug!("active categories: {:?}", selected_categories_manager.keys());
    }
}



impl LoadedPackTexture {
    const ACTIVATION_DATA_FILE_NAME: &'static str = "activation.json";
    
    pub fn category_set_all(&mut self, status: bool) {
        CategorySelection::recursive_set_all(&mut self.selectable_categories, status);
        self._is_dirty = true;
    }
    
    pub fn update_active_categories(&mut self, active_elements: &HashSet<Uuid>) {
        CategorySelection::recursive_update_active_categories(&mut self.selectable_categories, active_elements);
    }
    pub fn category_sub_menu(
        &mut self, 
        u2b_sender: &std::sync::mpsc::Sender<UIToBackMessage>,
        u2u_sender: &std::sync::mpsc::Sender<UIToUIMessage>,
        ui: &mut egui::Ui, 
        show_only_active: bool, 
    ) {
        //it is important to generate a new id each time to avoid collision
        ui.push_id(ui.next_auto_id(), |ui| {
            CategorySelection::recursive_selection_ui(
                u2b_sender,
                u2u_sender,
                &mut self.selectable_categories,
                ui,
                &mut self._is_dirty,
                show_only_active,
                &self.late_discovery_categories
            );
        });
        if self._is_dirty {
            u2b_sender.send(UIToBackMessage::CategoryActivationStatusChanged);
        }
    }

    pub fn is_dirty(&self) -> bool {
        self._is_dirty
    }
    pub fn tick(
        &mut self,
        u2u_sender: &std::sync::mpsc::Sender<UIToUIMessage>,
        _timestamp: f64,
        link: &MumbleLink,
        //next_on_screen: &mut HashSet<Uuid>,
        z_near: f32,
        tasks: &PackTasks,
    ) {
        tracing::trace!("LoadedPackTexture.tick: {} {}-{} {}-{}", 
            self.name,
            self.current_map_data.active_markers.len(), 
            self.current_map_data.wip_markers.len(), 
            self.current_map_data.active_trails.len(), 
            self.current_map_data.wip_trails.len(),
        );
        let mut marker_objects = Vec::new();
        for (uuid, marker) in self.current_map_data.active_markers.iter() {
            if let Some(mo) = marker.get_vertices_and_texture(link, z_near) {
                marker_objects.push(mo);
            }
        }
        tracing::trace!("LoadedPackTexture.tick: {}, markers {}", self.name, marker_objects.len());
        u2u_sender.send(UIToUIMessage::BulkMarkerObject(marker_objects));
        let mut trail_objects = Vec::new();
        for (uuid, trail) in self.current_map_data.active_trails.iter() {
            trail_objects.push(TrailObject {
                vertices: trail.trail_object.vertices.clone(),
                texture: trail.trail_object.texture,
            });
            //next_on_screen.insert(*uuid);
        }
        tracing::trace!("LoadedPackTexture.tick: {}, trails {}", self.name, trail_objects.len());
        u2u_sender.send(UIToUIMessage::BulkTrailObject(trail_objects));
    }

    pub fn swap(&mut self) {
        info!("swap {} to display {} textures, {} markers, {} trails", 
            self.name, 
            self.current_map_data.active_textures.len(),
            self.current_map_data.wip_markers.len(), 
            self.current_map_data.wip_trails.len()
        );
        self.current_map_data.active_markers = std::mem::take(&mut self.current_map_data.wip_markers);
        self.current_map_data.active_trails = std::mem::take(&mut self.current_map_data.wip_trails);
    }

    pub fn load_marker_texture(
        &mut self, 
        egui_context: &egui::Context, 
        default_tex_id: &TextureHandle,
        tex_path: &RelativePath,
        marker_uuid: Uuid,
        position: glam::Vec3,
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
                info!(%tex_path, "failed to find this icon texture");
            }
        }
        let th = self.current_map_data.active_textures.get(tex_path)
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
        self.current_map_data
            .wip_markers
            .insert(marker_uuid, am);
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
                info!(%tex_path, "failed to find this trail texture");
            }
        } else {
            debug!("Trail texture alreadu loaded {:?}", tex_path);
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
        if let Some(active_trail) = ActiveTrail::get_vertices_and_texture(
            &common_attributes,
            &tbin.nodes,
            th.clone(),
        ) {
            self.current_map_data
                .wip_trails
                .insert(trail_uuid, active_trail);
        } else {
            info!("Cannot display {texture_path:?}")
        }

    }

}

pub fn jokolay_to_marker_dir(jokolay_dir: &Arc<Dir>) -> Result<Dir> {
    jokolay_dir.create_dir_all(PACKAGE_MANAGER_DIRECTORY_NAME)
        .into_diagnostic()
        .wrap_err(format!("failed to create marker manager directory {}", PACKAGE_MANAGER_DIRECTORY_NAME))?;
    let marker_manager_dir = jokolay_dir
        .open_dir(PACKAGE_MANAGER_DIRECTORY_NAME)
        .into_diagnostic()
        .wrap_err(format!("failed to open marker manager directory {}", PACKAGE_MANAGER_DIRECTORY_NAME))?;
    marker_manager_dir
        .create_dir_all(PACKAGES_DIRECTORY_NAME)
        .into_diagnostic()
        .wrap_err(format!("failed to create marker packs directory {}", PACKAGES_DIRECTORY_NAME))?;
    let marker_packs_dir = marker_manager_dir
        .open_dir(PACKAGES_DIRECTORY_NAME)
        .into_diagnostic()
        .wrap_err(format!("failed to open marker packs dir {}", PACKAGES_DIRECTORY_NAME))?;
    Ok(marker_packs_dir)
}

pub fn load_all_from_dir(jokolay_dir: Arc<Dir>) -> Result<(BTreeMap<Uuid, LoadedPackData>, BTreeMap<Uuid, LoadedPackTexture>)>{
    let marker_packs_dir = jokolay_to_marker_dir(&jokolay_dir)?;
    let mut data_packs: BTreeMap<Uuid, LoadedPackData> = Default::default();
    let mut texture_packs: BTreeMap<Uuid, LoadedPackTexture> = Default::default();


    for entry in marker_packs_dir
        .entries()
        .into_diagnostic()
        .wrap_err("failed to get entries of marker packs dir")?
    {
        let entry = entry.into_diagnostic()?;
        if entry.metadata().into_diagnostic()?.is_file() {
            continue;
        }
        if let Ok(name) = entry.file_name() {
            let pack_dir = entry
                .open_dir()
                .into_diagnostic()
                .wrap_err(format!("failed to open pack entry as directory: {}", name))?;
            {
                let span_guard = info_span!("loading pack from dir", name).entered();

                match build_from_dir(name.clone(), pack_dir.into()) {
                    Ok(lp) => {
                        let (data, tex) = lp;
                        data_packs.insert(data.uuid, data);
                        texture_packs.insert(tex.uuid, tex);
                    }
                    Err(e) => {
                        error!(?e, "failed to load pack from directory: {}", name);
                    }
                }
                drop(span_guard);
            }
        }
    }
    Ok((data_packs, texture_packs))
}

fn build_from_dir(name: String, pack_dir: Arc<Dir>) -> Result<(LoadedPackData, LoadedPackTexture)> {
    if !pack_dir
        .try_exists(LoadedPackData::CORE_PACK_DIR_NAME)
        .into_diagnostic()
        .wrap_err("failed to check if pack core exists")?
    {
        bail!("pack core doesn't exist in this pack");
    }
    let core_dir = pack_dir
        .open_dir(LoadedPackData::CORE_PACK_DIR_NAME)
        .into_diagnostic()
        .wrap_err("failed to open core pack directory")?;
    let start = std::time::SystemTime::now();
    let core = load_pack_core_from_dir(&core_dir).wrap_err("failed to load pack from dir")?;
    let elaspsed = start.elapsed().unwrap_or_default();
    tracing::info!("Loading of package from disk {} took {} ms", name, elaspsed.as_millis());
    let res = build_from_core(name.clone(), pack_dir, core);
    Ok(res)
}


pub fn build_from_core(name: String, pack_dir: Arc<Dir>, core: PackCore) -> (LoadedPackData, LoadedPackTexture) {
    let selectable_categories = LoadedPackData::load_selectable_categories(&pack_dir, &core);
    let data = LoadedPackData {
        name: name.clone(),
        uuid: core.uuid,
        dir: Arc::clone(&pack_dir),
        selected_files: Default::default(),
        all_categories: core.all_categories,
        categories: core.categories,
        maps: core.maps,
        source_files: core.source_files,
        _is_dirty: false,
        activation_data: Default::default(),
        active_elements: Default::default(),
        selectable_categories: selectable_categories.clone(),
        entities_parents: core.entities_parents,
    };
    let activation_data = (if pack_dir.is_file(LoadedPackTexture::ACTIVATION_DATA_FILE_NAME) {
        match pack_dir.read_to_string(LoadedPackTexture::ACTIVATION_DATA_FILE_NAME) {
                Ok(contents) => match serde_json::from_str(&contents) {
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
        dir: Arc::clone(&pack_dir),
        late_discovery_categories: core.late_discovery_categories,
        name: name,
        tbins: core.tbins,
        active_elements: Default::default(),
    };
    (data, tex)
}

