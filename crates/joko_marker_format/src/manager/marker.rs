use std::{
    collections::BTreeMap, sync::{Arc, Mutex}, collections::HashSet
};

use tribool::Tribool;
use cap_std::fs_utf8::Dir;
use egui::{CollapsingHeader, ColorImage, TextureHandle, Window};
use image::EncodableLayout;

use tracing::{error, info, info_span};

use jokolink::MumbleLink;
use miette::{Context, IntoDiagnostic, Result};
use uuid::Uuid;

use crate::manager::pack::loaded::LoadedPack;
use crate::manager::pack::import::{ImportStatus, import_pack_from_zip_file_path};

pub const MARKER_MANAGER_DIRECTORY_NAME: &str = "marker_manager";
pub const MARKER_PACKS_DIRECTORY_NAME: &str = "packs";
// pub const MARKER_MANAGER_CONFIG_NAME: &str = "marker_manager_config.json";

/// It manage everything that has to do with marker packs.
/// 1. imports, loads, saves and exports marker packs.
/// 2. maintains the categories selection data for every pack
/// 3. contains activation data globally and per character
/// 4. When we load into a map, it filters the markers and runs the logic every frame
///     1. If a marker needs to be activated (based on player position or whatever)
///     2. marker needs to be drawn
///     3. marker's texture is uploaded or being uploaded? if not ready, we will upload or use a temporary "loading" texture
///     4. render that marker use joko_render  
/// FIXME: it is a bad name, it does not manage Markers, but packages
pub struct MarkerManager {
    /// holds data that is useful for the ui
    ui_data: MarkerManagerUI,
    /// marker manager directory. not useful yet, but in future we could be using this to store config files etc..
    _marker_manager_dir: Arc<Dir>,
    /// packs directory which contains marker packs. each directory inside pack directory is an individual marker pack.
    /// The name of the child directory is the name of the pack
    marker_packs_dir: Arc<Dir>,
    /// These are the marker packs
    /// The key is the name of the pack
    /// The value is a loaded pack that contains additional data for live marker packs like what needs to be saved or category selections etc..
    packs: BTreeMap<String, LoadedPack>,
    missing_texture: Option<TextureHandle>,
    missing_trail: Option<TextureHandle>,
    /// This is the interval in number of seconds when we check if any of the packs need to be saved due to changes.
    /// This allows us to avoid saving the pack too often.
    pub save_interval: f64,

    all_files_tribool: Tribool,
    all_files_toggle: bool,
    currently_used_files: BTreeMap<String, bool>,
    on_screen: HashSet<Uuid>,
    is_dirty: bool
}

#[derive(Debug, Default)]
pub(crate) struct MarkerManagerUI {
    // tf is this type supposed to be? maybe we should have used a ECS for this reason.
    pub import_status: Option<Arc<Mutex<ImportStatus>>>,
}


impl MarkerManager {
    /// Creates a new instance of [MarkerManager].
    /// 1. It opens the marker manager directory
    /// 2. loads its configuration
    /// 3. opens the packs directory
    /// 4. loads all the packs
    /// 5. loads all the activation data
    /// 6. returns self
    pub fn new(jdir: &Dir) -> Result<Self> {
        jdir.create_dir_all(MARKER_MANAGER_DIRECTORY_NAME)
            .into_diagnostic()
            .wrap_err("failed to create marker manager directory")?;
        let marker_manager_dir = jdir
            .open_dir(MARKER_MANAGER_DIRECTORY_NAME)
            .into_diagnostic()
            .wrap_err("failed to open marker manager directory")?;
        marker_manager_dir
            .create_dir_all(MARKER_PACKS_DIRECTORY_NAME)
            .into_diagnostic()
            .wrap_err("failed to create marker packs directory")?;
        let marker_packs_dir = marker_manager_dir
            .open_dir(MARKER_PACKS_DIRECTORY_NAME)
            .into_diagnostic()
            .wrap_err("failed to open marker packs dir")?;
        let mut packs: BTreeMap<String, LoadedPack> = Default::default();

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
                    .wrap_err("failed to open pack entry as directory")?;
                {
                    let span_guard = info_span!("loading pack from dir", name).entered();
                    match LoadedPack::load_from_dir(pack_dir.into()) {
                        Ok(lp) => {
                            packs.insert(name, lp);
                        }
                        Err(e) => {
                            error!(?e, "failed to load pack from directory");
                        }
                    }
                    drop(span_guard);
                }
            }
        }

        Ok(Self {
            packs,
            marker_packs_dir: marker_packs_dir.into(),
            _marker_manager_dir: marker_manager_dir.into(),
            ui_data: Default::default(),
            save_interval: 0.0,
            missing_texture: None,
            missing_trail: None,
            all_files_tribool: Tribool::True,
            all_files_toggle: false,
            currently_used_files: Default::default(),
            on_screen: Default::default(),
            is_dirty: true,
        })
    }

    fn pack_importer(import_status: Arc<Mutex<ImportStatus>>) {
        //called when a new pack is imported
        rayon::spawn(move || {
            *import_status.lock().unwrap() = ImportStatus::WaitingForFileChooser;

            if let Some(file_path) = rfd::FileDialog::new()
                .add_filter("taco", &["zip", "taco"])
                .pick_file()
            {
                *import_status.lock().unwrap() = ImportStatus::LoadingPack(file_path.clone());

                let result = import_pack_from_zip_file_path(file_path);
                match result {
                    Ok((name, pack)) => {
                        *import_status.lock().unwrap() = ImportStatus::PackDone(name, pack, false);
                    }
                    Err(e) => {
                        *import_status.lock().unwrap() = ImportStatus::PackError(e);
                    }
                }
            } else {
                *import_status.lock().unwrap() =
                    ImportStatus::PackError(miette::miette!("file chooser was cancelled"));
            }
        });
    }
    pub fn tick(
        &mut self,
        etx: &egui::Context,
        timestamp: f64,
        joko_renderer: &mut joko_render::JokoRenderer,
        link: Option<&MumbleLink>,
    ) {
        if self.missing_texture.is_none() {
            let img = image::load_from_memory(include_bytes!("../pack/marker.png")).unwrap();
            let size = [img.width() as _, img.height() as _];
            self.missing_texture = Some(etx.load_texture(
                "default marker",
                ColorImage::from_rgba_unmultiplied(size, img.into_rgba8().as_bytes()),
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    wrap_mode: egui::TextureWrapMode::ClampToEdge,
                },
            ));
        }
        if self.missing_trail.is_none() {
            let img = image::load_from_memory(include_bytes!("../pack/trail.png")).unwrap();
            let size = [img.width() as _, img.height() as _];
            self.missing_trail = Some(etx.load_texture(
                "default trail",
                ColorImage::from_rgba_unmultiplied(size, img.into_rgba8().as_bytes()),
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    wrap_mode: egui::TextureWrapMode::ClampToEdge,
                },
            ));
        }

        let mut currently_used_files: BTreeMap<String, bool> = Default::default();
        let mut next_on_screen: HashSet<Uuid> = Default::default();
        match link {
            Some(link) => {
                //FIXME: how to save/load the active files ?
                let mut is_dirty = self.is_dirty;
                for pack in self.packs.values_mut() {
                    if let Some(current_map) = pack.core.maps.get(&link.map_id) {
                        for marker in current_map.markers.values() {
                            if let Some(is_active) = pack.core.source_files.get(&marker.source_file_name) {
                                currently_used_files.insert(
                                    marker.source_file_name.clone(), 
                                    *self.currently_used_files.get(&marker.source_file_name).unwrap_or_else(|| {is_dirty = true; is_active})
                                );
                            }
                        }
                        for trail in current_map.trails.values() {
                            if let Some(is_active) = pack.core.source_files.get(&trail.source_file_name) {
                                currently_used_files.insert(
                                    trail.source_file_name.clone(), 
                                    *self.currently_used_files.get(&trail.source_file_name).unwrap_or_else(|| {is_dirty = true; is_active})
                                );
                            }
                        }
                    }
                }
                for pack in self.packs.values_mut() {
                    pack.tick(
                        etx,
                        timestamp,
                        link,
                        self.missing_texture.as_ref().unwrap(),
                        self.missing_trail.as_ref().unwrap(),
                        &currently_used_files,
                        is_dirty
                    );
                    pack.render(
                        timestamp,
                        joko_renderer,
                        link,
                        &mut next_on_screen,
                    );
                }
                std::mem::take(&mut self.is_dirty);
            },
            None => {},
        };
        self.currently_used_files = currently_used_files;
        self.on_screen = next_on_screen;//those are the elements displayed, not the categories, one would need to keep the link between the two
    }
    pub fn menu_ui(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Markers", |ui| {
            for pack in self.packs.values_mut() {
                pack.category_sub_menu(ui, &self.on_screen);
            }
        });
        
    }
    fn gui_file_manager(&mut self, etx: &egui::Context, open: &mut bool, link: Option<&MumbleLink>) {
        Window::new("File Manager").open(open).show(etx, |ui| -> Result<()> {
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui::Grid::new("link grid")
                    .num_columns(4)
                    .striped(true)
                    .show(ui, |ui| {
                        if self.all_files_tribool.is_indeterminate(){
                            ui.add(egui::Checkbox::new(&mut self.all_files_toggle, "File").indeterminate(true));
                        } else {
                            ui.checkbox(&mut self.all_files_toggle, "File");
                        }
                        ui.label("Trails");
                        ui.label("Markers");
                        ui.end_row();
                        
                        for file in self.currently_used_files.iter_mut() {
                            let cb = ui.checkbox(file.1, file.0.clone());
                            if cb.changed() {
                                self.is_dirty = true;
                            }
                            if ui.button("Edit").clicked() {
                                println!("click {}", file.0.clone());
                            }
                            ui.end_row();
                        }
                        ui.end_row();
                    })
            });
            Ok(())
        });
    }
    fn gui_marker_manager(&mut self, etx: &egui::Context, open: &mut bool) {
        Window::new("Marker Manager").open(open).show(etx, |ui| -> Result<()> {
            CollapsingHeader::new("Loaded Packs").show(ui, |ui| {
                egui::Grid::new("packs").striped(true).show(ui, |ui| {
                    let mut delete = vec![];
                for pack in self.packs.keys() {
                    ui.label(pack);
                    if ui.button("delete").clicked() {
                        delete.push(pack.clone());
                    }
                }
                for pack_name in delete {
                    self.packs.remove(&pack_name);
                    if let Err(e) = self.marker_packs_dir.remove_dir_all(&pack_name) {
                        error!(?e, pack_name,"failed to remove pack");
                    } else {
                        info!("deleted marker pack: {pack_name}");
                    }
                }
            });
            });

            if self.ui_data.import_status.is_some() {
                if ui.button("clear").on_hover_text(
                    "This will cancel any pack import in progress. If import is already finished, then it wil simply clear the import status").clicked() {
                    self.ui_data.import_status = None;
                }
            } else if ui.button("import pack").on_hover_text("select a taco/zip file to import the marker pack from").clicked() {
                let import_status = Arc::new(Mutex::default());
                self.ui_data.import_status = Some(import_status.clone());
                Self::pack_importer(import_status);
            }
            if let Some(import_status) = self.ui_data.import_status.as_ref() {
                if let Ok(mut status) = import_status.lock() {
                    match &mut *status {
                        ImportStatus::UnInitialized => {
                            ui.label("import not started yet");
                        }
                        ImportStatus::WaitingForFileChooser => {
                            ui.label(
                                "wailting for the file dialog. choose a taco/zip file to import",
                            );
                        }
                        ImportStatus::LoadingPack(p) => {
                            ui.label(format!("pack is being imported from {p:?}"));
                        }
                        ImportStatus::PackDone(name, pack, saved) => {

                            if !*saved {
                                ui.horizontal(|ui| {
                                    ui.label("choose a pack name: ");    
                                    ui.text_edit_singleline(name);
                                });
                                let name = name.as_str();
                                if ui.button("save").clicked() {

                                    if self.marker_packs_dir.exists(name) {
                                        self.marker_packs_dir
                                            .remove_dir_all(name)
                                            .into_diagnostic()?;
                                    }
                                    if let Err(e) = self.marker_packs_dir.create_dir_all(name) {
                                        error!(?e, "failed to create directory for pack");

                                    }
                                    match self.marker_packs_dir.open_dir(name) {
                                        Ok(dir) => {
                                            let core = std::mem::take(pack);
                                            let mut loaded_pack = LoadedPack::new(core, dir.into());
                                            match loaded_pack.save_all() {
                                                Ok(_) => {
                                                    self.packs.insert(name.to_string(), loaded_pack);
                                                    *saved = true;
                                                },
                                                Err(e) => {
                                                    error!(?e, "failed to save marker pack");
                                                },
                                            }
                                        },
                                        Err(e) => {
                                            error!(?e, "failed to open marker pack directory to save pack");
                                        }
                                    };
                                }
                            } else {
                                ui.colored_label(egui::Color32::GREEN, "pack is saved. press click `clear` button to remove this message");
                            }
                        }
                        ImportStatus::PackError(e) => {
                            ui.colored_label(
                                egui::Color32::RED,
                                format!("failed to import pack due to error: {e:#?}"),
                            );
                        }
                    }
                }
            }

            Ok(())
        });
    }
    pub fn gui(
        &mut self, 
        etx: &egui::Context, 
        is_marker_open: &mut bool, 
        is_file_open: &mut bool, 
        timestamp: f64,
        joko_renderer: &mut joko_render::JokoRenderer,
        link: Option<&MumbleLink>
    ) {
        self.gui_marker_manager(etx, is_marker_open);
        self.gui_file_manager(etx, is_file_open, link);
}
}

