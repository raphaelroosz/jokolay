use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet}, sync::Arc
};
use ordered_hash_map::{OrderedHashMap};

use cap_std::fs_utf8::Dir;
use egui::{ColorImage, TextureHandle};
use image::{EncodableLayout};
use joko_render::billboard::{TrailObject};
use tracing::{debug, error, info};
use uuid::Uuid;

use crate::{
    io::{load_pack_core_from_dir, save_pack_core_to_dir}, manager::pack::{category_selection::SelectedCategoryManager, file_selection::SelectedFileManager}, pack::{PackCore}
};
use jokolink::MumbleLink;
use miette::{bail, Context, IntoDiagnostic, Result};

use super::activation::{ActivationData, ActivationType};
use super::active::{CurrentMapData, ActiveMarker, ActiveTrail};
use crate::manager::pack::category_selection::CategorySelection;

pub(crate) struct LoadedPack {
    /// The directory inside which the pack data is stored
    /// There should be a subdirectory called `core` which stores the pack core
    /// Files related to Jokolay thought will have to be stored directly inside this directory, to keep the xml subdirectory clean.
    /// eg: Active categories, activation data etc..
    pub dir: Arc<Dir>,
    /// The actual xml pack.
    pub core: PackCore,
    /// The selection of categories which are "enabled" and markers belonging to these may be rendered
    selected_categories: OrderedHashMap<String, CategorySelection>,
    selected_files: OrderedHashMap<String, bool>,
    is_dirty: bool,
    activation_data: ActivationData,
    current_map_data: CurrentMapData,
}

impl LoadedPack {
    const CORE_PACK_DIR_NAME: &'static str = "core";
    const CATEGORY_SELECTION_FILE_NAME: &'static str = "cats.json";
    const ACTIVATION_DATA_FILE_NAME: &'static str = "activation.json";

    pub fn new(core: PackCore, dir: Arc<Dir>) -> Self {
        let selected_categories = CategorySelection::default_from_pack_core(&core);
        LoadedPack {
            dir,
            core,
            selected_categories,
            selected_files: Default::default(),
            is_dirty: false,
            activation_data: Default::default(),
            current_map_data: Default::default(),
        }
    }
    pub fn category_sub_menu(&mut self, ui: &mut egui::Ui, on_screen: &BTreeSet<Uuid>) {
        //it is important to generate a new id each time to avoid collision
        ui.push_id(ui.next_auto_id(), |ui| {
            CategorySelection::recursive_selection_ui(
                &mut self.selected_categories,
                ui,
                &mut self.is_dirty,
                on_screen
            );
        });
    }
    pub fn load_from_dir(pack_dir: Arc<Dir>) -> Result<Self> {
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
        let core = load_pack_core_from_dir(&core_dir).wrap_err("failed to load pack from dir")?;

        //FIXME: Since categories have randomly generated uuids (and not saved), one need to build from those, all the time.
        let selected_categories = CategorySelection::default_from_pack_core(&core);
        /***
        FIXME: Once this is saved  properly, we can restore following block of code.

        let selected_categories = (if pack_dir.is_file(Self::CATEGORY_SELECTION_FILE_NAME) {
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
            let cs = CategorySelection::default_from_pack_core(&core);
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
        });
        **/
        let activation_data = (if pack_dir.is_file(Self::ACTIVATION_DATA_FILE_NAME) {
            match pack_dir.read_to_string(Self::ACTIVATION_DATA_FILE_NAME) {
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
        Ok(LoadedPack {
            dir: pack_dir,
            core,
            selected_categories,
            selected_files: Default::default(),
            is_dirty: false,
            activation_data,
            current_map_data: Default::default(),
        })
    }

    pub fn tick(
        &mut self,
        etx: &egui::Context,
        _timestamp: f64,
        link: &MumbleLink,
        default_tex_id: &TextureHandle,
        default_trail_id: &TextureHandle,
        currently_used_files: &BTreeMap<String, bool>,
        is_dirty: bool,
    ) {
        let is_dirty = self.is_dirty || is_dirty;
        if self.is_dirty {
            match self.save() {
                Ok(_) => {}
                Err(e) => {
                    error!(?e, "failed to save marker pack");
                }
            }
        }
        //FIXME: takes a lot of time when "is_dirty" is true (i.e.: the map of things to display changes). Everythings get reloaded => how to do partial version ?
        if self.current_map_data.map_id != link.map_id || is_dirty {
            self.on_map_changed(etx, link, default_tex_id, default_trail_id, currently_used_files);
        }
    }
    pub fn render(
        &mut self,
        _timestamp: f64,
        joko_renderer: &mut joko_render::JokoRenderer,
        link: &MumbleLink,
        next_on_screen: &mut HashSet<Uuid>,
    ) {
        let z_near = joko_renderer.get_z_near();
        for (uuid, marker) in self.current_map_data.active_markers.iter() {
            //FIXME: what's the difference between a Marker and an ActiveMarker ? rename second one in something more fitting ?
            if let Some(mo) = marker.get_vertices_and_texture(link, z_near) {
                joko_renderer.add_billboard(mo);
                next_on_screen.insert(*uuid);
            }
        }
        for (uuid, trail) in self.current_map_data.active_trails.iter() {
            joko_renderer.add_trail(TrailObject {
                vertices: trail.trail_object.vertices.clone(),
                texture: trail.trail_object.texture,
            });
            next_on_screen.insert(*uuid);
        }
    }
    fn on_map_changed(
        &mut self,
        etx: &egui::Context,
        link: &MumbleLink,
        default_tex_id: &TextureHandle,
        default_trail_id: &TextureHandle,
        currently_used_files: &BTreeMap<String, bool>,
    ) {
        info!(
            self.current_map_data.map_id,
            link.map_id, "current map data is updated."
        );
        self.current_map_data = Default::default();
        if link.map_id == 0 {
            info!("No map do not do anything");
            return;
        }
        self.current_map_data.map_id = link.map_id;
        //CategorySelection::recursive_populate_guids(&mut self.selected_categories, &mut self.core.entities_parents, None);
        let selected_categories_manager = SelectedCategoryManager::new(&self.selected_categories, &self.core.categories);

        let selected_files_manager = SelectedFileManager::new(&self.selected_files, &self.core.source_files, &currently_used_files);
        
        let mut failure_loading = false;
        let mut nb_markers_attempt = 0;
        let mut nb_markers_loaded = 0;
        for (index, marker) in self
            .core
            .maps
            .get(&link.map_id)
            .unwrap_or(&Default::default())
            .markers
            .values()
            .enumerate()
        {
            nb_markers_attempt += 1;
            if selected_files_manager.is_selected(&marker.source_file_name) {
                if selected_categories_manager.is_selected(&marker.category) {
                    let category_attributes = selected_categories_manager.get(&marker.category);
                    let mut attrs = marker.attrs.clone();// why a clone ?
                    attrs.inherit_if_attr_none(category_attributes);
                    let key = &marker.guid;
                    if let Some(behavior) = attrs.get_behavior() {
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
                    if let Some(tex_path) = attrs.get_icon_file() {
                        if !self.current_map_data.active_textures.contains_key(tex_path) {
                            if let Some(tex) = self.core.textures.get(tex_path) {
                                let img = image::load_from_memory(tex).unwrap();
                                self.current_map_data.active_textures.insert(
                                    tex_path.clone(),
                                    etx.load_texture(
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
                                failure_loading = true;
                            }
                        }
                    } else {
                        info!("no texture attribute on this marker");
                    }
                    let th = attrs
                        .get_icon_file()
                        .and_then(|path| self.current_map_data.active_textures.get(path))
                        .unwrap_or(default_tex_id);
                    let texture_id = match th.id() {
                        egui::TextureId::Managed(i) => i,
                        egui::TextureId::User(_) => todo!(),
                    };

                    let max_pixel_size = attrs.get_max_size().copied().unwrap_or(2048.0); // default taco max size
                    let min_pixel_size = attrs.get_min_size().copied().unwrap_or(5.0); // default taco min size
                    self.current_map_data.active_markers.insert(
                        marker.guid,
                        ActiveMarker {
                            texture_id,
                            _texture: th.clone(),
                            attrs,
                            pos: marker.position,
                            max_pixel_size,
                            min_pixel_size,
                        },
                    );
                    nb_markers_loaded += 1;
                }
            }
        }

        let mut nb_trails_attempt = 0;
        let mut nb_trails_loaded = 0;
        for (index, trail) in self
            .core
            .maps
            .get(&link.map_id)
            .unwrap_or(&Default::default())
            .trails
            .values()
            .enumerate()
        {
            nb_trails_attempt += 1;
            if selected_files_manager.is_selected(&trail.source_file_name) {
                if selected_categories_manager.is_selected(&trail.category) {
                    let category_attributes = selected_categories_manager.get(&trail.category);
                    let mut common_attributes = trail.props.clone();
                    common_attributes.inherit_if_attr_none(category_attributes);
                    if let Some(tex_path) = common_attributes.get_texture() {
                        if !self.current_map_data.active_textures.contains_key(tex_path) {
                            if let Some(tex) = self.core.textures.get(tex_path) {
                                let img = image::load_from_memory(tex).unwrap();
                                self.current_map_data.active_textures.insert(
                                    tex_path.clone(),
                                    etx.load_texture(
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
                                failure_loading = true;
                            }
                        } else {
                            debug!("Trail texture alreadu loaded {:?}", tex_path);
                        }
                    } else {
                        info!("no texture attribute on this trail");
                    }
                    let texture_path = common_attributes.get_texture();
                    let th = texture_path
                        .and_then(|path| self.current_map_data.active_textures.get(path))
                        .unwrap_or(default_trail_id);

                    let tbin_path = if let Some(tbin) = common_attributes.get_trail_data() {
                        debug!(?texture_path, "tbin path");
                        tbin
                    } else {
                        info!(?trail, "missing tbin path");
                        continue;
                    };
                    let tbin = if let Some(tbin) = self.core.tbins.get(tbin_path) {
                        tbin
                    } else {
                        info!(%tbin_path, "failed to find tbin");
                        continue;
                    };
                    //TODO: if iso and closed, split it as a polygon and fill it as a surface
                    if let Some(active_trail) = ActiveTrail::get_vertices_and_texture(
                        &common_attributes,
                        &tbin.nodes,
                        th.clone(),
                    ) {
                        self.current_map_data
                            .active_trails
                            .insert(trail.guid, active_trail);
                    } else {
                        info!("Cannot display {texture_path:?}")
                    }
                    nb_trails_loaded += 1;
                } else {
                    info!("category {} is not enabled", trail.category);
                }
            }
        }
        info!("Loaded for {}: {}/{} markers and {}/{} trails", link.map_id, nb_markers_loaded, nb_markers_attempt, nb_trails_loaded, nb_trails_attempt);
        debug!("active categories: {:?}", selected_categories_manager.keys());

        if failure_loading {
            info!("Error when loading textures, here are the keys:");
            for k in self.core.textures.keys() {
                info!(%k);
            }
            info!("end of keys");
        }
    }
    pub fn save_all(&mut self) -> Result<()> {
        self.is_dirty = true;
        self.save()
    }
    #[tracing::instrument(skip(self))]
    pub fn save(&mut self) -> Result<()> {
        let is_dirty = std::mem::take(&mut self.is_dirty);
        if is_dirty {
            match serde_json::to_string_pretty(&self.selected_categories) {
                Ok(cs_json) => match self.dir.write(Self::CATEGORY_SELECTION_FILE_NAME, cs_json) {
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
            match serde_json::to_string_pretty(&self.activation_data) {
                Ok(ad_json) => match self.dir.write(Self::ACTIVATION_DATA_FILE_NAME, ad_json) {
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
        }
        self.dir
            .create_dir_all(Self::CORE_PACK_DIR_NAME)
            .into_diagnostic()
            .wrap_err("failed to create xmlpack directory")?;
        let core_dir = self
            .dir
            .open_dir(Self::CORE_PACK_DIR_NAME)
            .into_diagnostic()
            .wrap_err("failed to open core pack directory")?;
        save_pack_core_to_dir(
            &self.core,
            &core_dir,
            is_dirty,
        )?;
        Ok(())
    }
}
