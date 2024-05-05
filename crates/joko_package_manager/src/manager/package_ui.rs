use std::{
    borrow::BorrowMut,
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
};

use egui::{CollapsingHeader, ColorImage, TextureHandle, Ui, Window};
use image::EncodableLayout;
use joko_package_models::{attributes::CommonAttributes, package::PackageImportReport};

use joko_render_models::messages::MessageToRenderer;
use joko_ui_models::{UIArea, UIPanel};
use serde::{Deserialize, Serialize};
use tracing::{info_span, trace};

use crate::message::MessageToPackageBack;
use joko_component_models::{
    from_broadcast, from_data, to_broadcast, to_data, Component, ComponentChannels,
    ComponentMessage, ComponentResult,
};
use joko_core::{serde_glam::Vec3, RelativePath};
use joko_link_models::{MumbleChanges, MumbleLink, MumbleLinkResult};
use miette::Result;
use uuid::Uuid;

use crate::manager::pack::import::ImportStatus;
use crate::manager::pack::loaded::{LoadedPackTexture, PackTasks};
use crate::message::MessageToPackageUI;

//FIXME: there is an interest to merge the PackageUIManager and the render
#[derive(Clone, Serialize, Deserialize)]
pub struct PackageUISharedState {
    list_of_textures_changed: bool, //Meant as an optimisation to only update when choice_of_category_changed have produced the list of textures to display
    first_load_done: bool,
    nb_running_tasks_on_back: i32, // store the number of running tasks in background thread
    import_status: Arc<Mutex<ImportStatus>>,
}

struct PackageUIChannels {
    subscription_mumblelink: tokio::sync::broadcast::Receiver<ComponentResult>,

    back_end_notifier: tokio::sync::mpsc::Sender<ComponentMessage>,
    back_end_receiver: tokio::sync::mpsc::Receiver<ComponentMessage>,
    renderer_notifier: tokio::sync::mpsc::Sender<ComponentMessage>,
}

#[must_use]
pub struct PackageUIManager {
    default_marker_texture: Option<TextureHandle>,
    default_trail_texture: Option<TextureHandle>,
    packs: BTreeMap<Uuid, LoadedPackTexture>,
    reports: BTreeMap<Uuid, PackageImportReport>,
    tasks: PackTasks,

    egui_context: Arc<egui::Context>,
    z_near: f32,
    currently_used_files: BTreeMap<Uuid, bool>,
    all_files_activation_status: bool, // this consume a change of display event
    show_only_active: bool,
    pack_details: Option<Uuid>, // if filled, display the details of the package

    delayed_marker_texture: Vec<(Uuid, RelativePath, Uuid, Vec3, CommonAttributes)>,
    delayed_trail_texture: Vec<(Uuid, RelativePath, Uuid, CommonAttributes)>,

    channels: Option<PackageUIChannels>,
    state: PackageUISharedState,
}

impl PackageUIManager {
    pub fn new(egui_context: Arc<egui::Context>, z_near: f32) -> Self {
        //z_near is a constant, make it a https://docs.rs/tokio/latest/tokio/sync/watch/index.html if required to be dynamic
        let state = PackageUISharedState {
            list_of_textures_changed: false,
            first_load_done: false,
            nb_running_tasks_on_back: 0,
            import_status: Default::default(),
        };
        let mut res = Self {
            packs: Default::default(),
            tasks: PackTasks::new(),
            reports: Default::default(),
            default_marker_texture: None,
            default_trail_texture: None,

            egui_context,
            z_near,
            all_files_activation_status: false,
            show_only_active: true,
            currently_used_files: Default::default(), // UI copy to (de-)activate files
            pack_details: None,

            delayed_marker_texture: Default::default(),
            delayed_trail_texture: Default::default(),
            channels: None,
            state,
        };
        res._init();
        res
    }

    fn handle_message(&mut self, msg: MessageToPackageUI) {
        match msg {
            MessageToPackageUI::ActiveElements(active_elements) => {
                tracing::trace!("Handling of MessageToPackageUI::ActiveElements");
                self.update_active_categories(&active_elements);
            }
            MessageToPackageUI::CurrentlyUsedFiles(currently_used_files) => {
                tracing::trace!("Handling of MessageToPackageUI::CurrentlyUsedFiles");
                self.set_currently_used_files(currently_used_files);
            }
            MessageToPackageUI::DeletedPacks(to_delete) => {
                tracing::trace!("Handling of MessageToPackageUI::DeletedPacks");
                self.delete_packs(to_delete);
            }
            MessageToPackageUI::FirstLoadDone => {
                let channels = self.channels.as_ref().unwrap();
                let renderer_notifier = &channels.renderer_notifier;
                let _ =
                    renderer_notifier.blocking_send(to_data(MessageToRenderer::RenderSwapChain));
                self.state.first_load_done = true;
            }
            MessageToPackageUI::ImportedPack(file_name, pack) => {
                tracing::trace!("Handling of MessageToPackageUI::ImportedPack");
                *self.state.import_status.lock().unwrap() =
                    ImportStatus::PackDone(file_name, pack, false);
            }
            MessageToPackageUI::ImportFailure(message) => {
                tracing::trace!("Handling of MessageToPackageUI::ImportFailure");
                *self.state.import_status.lock().unwrap() = ImportStatus::PackError(message);
            }
            MessageToPackageUI::LoadedPack(pack_texture, report) => {
                tracing::trace!("Handling of MessageToPackageUI::LoadedPack");
                self.save(pack_texture, report);
                self.state.import_status = Default::default();
                let channels = self.channels.as_mut().unwrap();
                let _ = channels.back_end_notifier.blocking_send(to_data(
                    MessageToPackageBack::CategoryActivationStatusChanged,
                ));
                let renderer_notifier = &channels.renderer_notifier;
                let _ =
                    renderer_notifier.blocking_send(to_data(MessageToRenderer::RenderSwapChain));
            }
            MessageToPackageUI::MarkerTexture(
                pack_uuid,
                tex_path,
                marker_uuid,
                position,
                common_attributes,
            ) => {
                tracing::trace!("Handling of MessageToPackageUI::MarkerTexture");
                //FIXME: make it a TODO on tick()
                self.delayed_marker_texture.push((
                    pack_uuid,
                    tex_path,
                    marker_uuid,
                    position,
                    common_attributes,
                ));
            }
            MessageToPackageUI::NbTasksRunning(nb_tasks) => {
                tracing::trace!("Handling of MessageToPackageUI::NbTasksRunning");
                self.state.nb_running_tasks_on_back = nb_tasks;
            }
            MessageToPackageUI::PackageActiveElements(pack_uuid, active_elements) => {
                tracing::trace!("Handling of MessageToPackageUI::PackageActiveElements");
                self.update_pack_active_categories(pack_uuid, &active_elements);
            }
            MessageToPackageUI::TextureSwapChain => {
                tracing::debug!("Handling of MessageToPackageUI::TextureSwapChain");
                self.swap();
                self.state.list_of_textures_changed = true;
            }
            MessageToPackageUI::TrailTexture(
                pack_uuid,
                tex_path,
                trail_uuid,
                common_attributes,
            ) => {
                tracing::trace!("Handling of MessageToPackageUI::TrailTexture");
                self.delayed_trail_texture.push((
                    pack_uuid,
                    tex_path,
                    trail_uuid,
                    common_attributes,
                ));
            }
            #[allow(unreachable_patterns)]
            _ => {
                unimplemented!("Handling MessageToPackageUI has not been implemented yet");
            }
        }
    }

    fn _init(&mut self) {
        let egui_context: &egui::Context = &self.egui_context;
        //TODO: make it even later, at another place
        if self.default_marker_texture.is_none() {
            let img = image::load_from_memory(include_bytes!("../../images/marker.png")).unwrap();
            let size = [img.width() as _, img.height() as _];
            self.default_marker_texture = Some(egui_context.load_texture(
                "default marker",
                ColorImage::from_rgba_unmultiplied(size, img.into_rgba8().as_bytes()),
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    wrap_mode: egui::TextureWrapMode::ClampToEdge,
                },
            ));
        }
        if self.default_trail_texture.is_none() {
            let img =
                image::load_from_memory(include_bytes!("../../images/trail_rainbow.png")).unwrap();
            let size = [img.width() as _, img.height() as _];
            self.default_trail_texture = Some(egui_context.load_texture(
                "default trail",
                ColorImage::from_rgba_unmultiplied(size, img.into_rgba8().as_bytes()),
                egui::TextureOptions {
                    magnification: egui::TextureFilter::Linear,
                    minification: egui::TextureFilter::Linear,
                    wrap_mode: egui::TextureWrapMode::ClampToEdge,
                },
            ));
        }
    }

    pub fn delete_packs(&mut self, to_delete: Vec<Uuid>) {
        for uuid in to_delete {
            self.packs.remove(&uuid);
            self.reports.remove(&uuid);
        }
    }
    pub fn set_currently_used_files(&mut self, currently_used_files: BTreeMap<Uuid, bool>) {
        self.currently_used_files = currently_used_files;
    }

    pub fn update_active_categories(&mut self, active_elements: &HashSet<Uuid>) {
        trace!("There are {} active elements", active_elements.len());
        for pack in self.packs.values_mut() {
            pack.update_active_categories(active_elements);
        }
    }

    pub fn update_pack_active_categories(
        &mut self,
        pack_uuid: Uuid,
        active_elements: &HashSet<Uuid>,
    ) {
        trace!("There are {} active elements", active_elements.len());
        for (uuid, pack) in self.packs.iter_mut() {
            if uuid == &pack_uuid {
                pack.update_active_categories(active_elements);
                break;
            }
        }
    }
    pub fn swap(&mut self) {
        for pack in self.packs.values_mut() {
            pack.swap();
        }
    }

    pub fn load_marker_texture(
        &mut self,
        pack_uuid: Uuid,
        egui_context: &egui::Context,
        tex_path: RelativePath,
        marker_uuid: Uuid,
        position: Vec3,
        common_attributes: CommonAttributes,
    ) {
        if let Some(pack) = self.packs.get_mut(&pack_uuid) {
            pack.load_marker_texture(
                egui_context,
                self.default_marker_texture.as_ref().unwrap(),
                &tex_path,
                marker_uuid,
                position,
                common_attributes,
            );
        };
    }
    pub fn load_trail_texture(
        &mut self,
        pack_uuid: Uuid,
        egui_context: &egui::Context,
        tex_path: RelativePath,
        trail_uuid: Uuid,
        common_attributes: CommonAttributes,
    ) {
        if let Some(pack) = self.packs.get_mut(&pack_uuid) {
            pack.load_trail_texture(
                egui_context,
                self.default_trail_texture.as_ref().unwrap(),
                &tex_path,
                trail_uuid,
                common_attributes,
            );
        };
    }

    fn pack_importer(import_status: Arc<Mutex<ImportStatus>>) {
        //called when a new pack is imported
        rayon::spawn(move || {
            *import_status.lock().unwrap() = ImportStatus::WaitingForFileChooser;

            if let Some(file_path) = rfd::FileDialog::new()
                .add_filter("taco", &["zip", "taco"])
                .pick_file()
            {
                *import_status.lock().unwrap() = ImportStatus::LoadingPack(file_path);
            } else {
                *import_status.lock().unwrap() =
                    ImportStatus::PackError("file chooser was cancelled".to_string());
            }
        });
    }

    fn category_set_all(&mut self, status: bool) {
        for pack in self.packs.values_mut() {
            pack.category_set_all(status);
        }
    }

    pub fn _tick(&mut self, timestamp: f64, link: &MumbleLink, z_near: f32) -> Result<()> {
        trace!("PackageUIManager::_tick for {} packages", self.packs.len());
        let tasks = &self.tasks;
        let channels = self.channels.as_ref().unwrap();
        let renderer_notifier = &channels.renderer_notifier;
        for pack in self.packs.values_mut() {
            tasks.save_texture(pack, pack.is_dirty());
        }
        if link.changes.contains(MumbleChanges::Position)
            || link.changes.contains(MumbleChanges::Map)
            || self.state.list_of_textures_changed
        {
            for pack in self.packs.values_mut() {
                let span_guard = info_span!("Updating package status").entered();
                pack.tick(renderer_notifier, timestamp, link, z_near, tasks)?;
                std::mem::drop(span_guard);
            }
            let _ = renderer_notifier.blocking_send(to_data(MessageToRenderer::RenderSwapChain));
            self.state.list_of_textures_changed = false;
        }
        Ok(())
    }

    pub fn status_as_color(
        &self,
        nb_running_tasks_on_back: i32,
        nb_running_tasks_on_network: i32,
    ) -> egui::Color32 {
        //we can choose whatever color code we want to focus on load, save, network queries, anything.
        let nb_running_tasks_on_ui = self.tasks.count();
        //Integer overflow avoidance example: value * 0x80 / 4 <=> value * 0x20
        let color_ui = if nb_running_tasks_on_ui > 0 {
            let nb_ui_tasks = nb_running_tasks_on_ui.clamp(0, 1) as u8;
            let res = nb_ui_tasks * 0x80;
            res + 0x7f
        } else {
            0
        };

        let color_back = if nb_running_tasks_on_back > 0 {
            let nb_bask_tasks = nb_running_tasks_on_back.clamp(0, 1) as u8;
            let res = nb_bask_tasks * 0x80;
            res + 0x7f
        } else {
            0
        };

        let color_network = if nb_running_tasks_on_network > 0 {
            let nb_network_tasks = nb_running_tasks_on_network.clamp(0, 1) as u8;
            let res = nb_network_tasks * 0x80;
            res + 0x7f
        } else {
            0
        };

        egui::Color32::from_rgb(color_ui, color_back, color_network)
    }

    fn gui_file_manager(&mut self, is_open: &mut bool) {
        //FIXME: the deactivate all for all files, seems to toggle only the next one not in target state
        let egui_context = self.egui_context.borrow_mut();
        let channels = self.channels.as_mut().unwrap();
        let mut files_changed = false;
        Window::new("File Manager")
            .open(is_open)
            .show(egui_context, |ui| -> Result<()> {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("link grid")
                        .num_columns(4)
                        .striped(true)
                        .show(ui, |ui| {
                            let mut all_files_toggle = false;
                            ui.horizontal(|ui| {
                                if ui.button("activate all").clicked() {
                                    self.all_files_activation_status = true;
                                    all_files_toggle = true;
                                    files_changed = true;
                                }
                                if ui.button("deactivate all").clicked() {
                                    self.all_files_activation_status = false;
                                    all_files_toggle = true;
                                    files_changed = true;
                                }
                            });
                            //ui.label("Trails");
                            //ui.label("Markers");
                            ui.end_row();

                            for pack in self.packs.values_mut() {
                                //TODO: first loop to list what is active per pack, to not display all packs
                                let report = self.reports.get(&pack.uuid).unwrap();
                                let mut pack_files_toggle = false;
                                let mut pack_files_activation_status = true;
                                ui.horizontal(|ui| {
                                    ui.label(&pack.name);
                                    if ui.button("activate all").clicked() {
                                        pack_files_activation_status = true;
                                        pack_files_toggle = true;
                                        files_changed = true;
                                    }
                                    if ui.button("deactivate all").clicked() {
                                        pack_files_activation_status = false;
                                        pack_files_toggle = true;
                                        files_changed = true;
                                    }
                                });
                                ui.end_row();
                                for source_file_uuid in pack.source_files.keys() {
                                    if let Some(is_selected) =
                                        self.currently_used_files.get_mut(source_file_uuid)
                                    {
                                        if all_files_toggle {
                                            *is_selected = self.all_files_activation_status;
                                        }
                                        if pack_files_toggle {
                                            *is_selected = pack_files_activation_status;
                                        }
                                        ui.add_space(3.0);
                                        //reports may be corrupted or not loaded, files are there
                                        if let Some(source_file_name) =
                                            report.source_file_uuid_to_name(source_file_uuid)
                                        {
                                            //format the file from reports and packages + prefix with the package name
                                            let cb = ui.checkbox(
                                                is_selected,
                                                format!("{}: {}", pack.name, source_file_name),
                                            );
                                            if cb.changed() {
                                                files_changed = true;
                                            }
                                        } else {
                                            // Import report is corrupted, only print reference
                                            let cb = ui.checkbox(
                                                is_selected,
                                                format!("{}: {}", pack.name, source_file_uuid),
                                            );
                                            if cb.changed() {
                                                files_changed = true;
                                            }
                                        }
                                        ui.end_row();
                                    }
                                }
                            }
                            ui.end_row();
                        })
                });
                Ok(())
            });
        if files_changed {
            let _ = channels.back_end_notifier.blocking_send(to_data(
                MessageToPackageBack::ActiveFiles(self.currently_used_files.clone()),
            ));
        }
    }

    fn gui_package_details(ui: &mut Ui, data: (&LoadedPackTexture, &PackageImportReport)) {
        // protection against deletion while displaying details
        let (pack, report) = data;

        let collapsing =
            CollapsingHeader::new(format!("Last load details of package {}", pack.name));
        //FIXME: clear the pack details
        let _header_response = collapsing
            .open(Some(true))
            .show(ui, |ui| {
                egui::Grid::new("packs details")
                    .striped(true)
                    .show(ui, |ui| {
                        let number_of = &report.number_of;
                        ui.label("categories");
                        ui.label(format!("{}", number_of.categories));
                        ui.end_row();
                        ui.label("missing_categories");
                        ui.label(format!("{}", number_of.missing_categories));
                        ui.end_row();
                        ui.label("textures");
                        ui.label(format!("{}", number_of.textures));
                        ui.end_row();
                        ui.label("missing_textures");
                        ui.label(format!("{}", number_of.missing_textures));
                        ui.end_row();
                        ui.label("entities");
                        ui.label(format!("{}", number_of.entities));
                        ui.end_row();
                        ui.label("markers");
                        ui.label(format!("{}", number_of.markers));
                        ui.end_row();
                        ui.label("trails");
                        ui.label(format!("{}", number_of.trails));
                        ui.end_row();
                        ui.label("routes");
                        ui.label(format!("{}", number_of.routes));
                        ui.end_row();
                        ui.label("maps");
                        ui.label(format!("{}", number_of.maps));
                        ui.end_row();
                        ui.label("source_files");
                        ui.label(format!("{}", number_of.source_files));
                        ui.end_row();
                    })
            })
            .header_response;
        /*if header_response.clicked() {
            self.pack_details = None;
        }*/
    }
    fn gui_package_list(&mut self, is_open: &mut bool) {
        let egui_context = self.egui_context.borrow_mut();
        let import_status = self.state.import_status.clone();
        let details = if let Some(uuid) = self.pack_details {
            if let Some(pack) = self.packs.get(&uuid) {
                if let Some(report) = self.reports.get(&uuid) {
                    Some((pack, report))
                } else {
                    self.pack_details = None;
                    None
                }
            } else {
                self.pack_details = None;
                None
            }
        } else {
            None
        };
        Window::new("Package Loader").open(is_open).show(egui_context, |ui| -> Result<()> {
            let channels = self.channels.as_mut().unwrap();
            if !self.state.first_load_done {
                ui.label("Loading in progress...");
            } else {
                CollapsingHeader::new("Loaded Packs").show(ui, |ui| {
                    egui::Grid::new("packs").striped(true).show(ui, |ui| {
                        let mut to_delete = vec![];
                        for pack in self.packs.values() {
                            ui.label(pack.name.clone());
                            if ui.button("delete").clicked() {
                                to_delete.push(pack.uuid);
                            }
                            if ui.button("Details").clicked() {
                                self.pack_details = Some(pack.uuid);
                            }
                            if ui.button("Export").clicked() {
                                //TODO
                            }
                            ui.end_row();
                        }
                        if !to_delete.is_empty() {
                            let _ = channels.back_end_notifier.blocking_send(to_data(MessageToPackageBack::DeletePacks(to_delete)));
                        }
                    });
                });
                if let Some(data) = details {
                    Self::gui_package_details(ui, data);
                } else if let Ok(mut status) = import_status.lock() {
                    match &mut *status {
                        ImportStatus::UnInitialized => {
                            if ui.button("import pack").on_hover_text("select a taco/zip file to import the marker pack from").clicked() {
                                Self::pack_importer(Arc::clone(&import_status));
                            }
                            //ui.label("import not started yet");
                        }
                        ImportStatus::WaitingForFileChooser => {
                            ui.label(
                                "waiting for the file dialog. choose a taco/zip file to import",
                            );
                        }
                        ImportStatus::LoadingPack(p) | ImportStatus::WaitingLoading(p) => {
                            ui.label(format!("pack is being imported from {p:?}"));
                        }
                        ImportStatus::PackDone(name, pack, saved) => {
                            if *saved {
                                ui.colored_label(egui::Color32::GREEN, "pack is saved. press click `clear` button to remove this message");
                            } else {
                                ui.horizontal(|ui| {
                                    ui.label("choose a pack name: ");    
                                    ui.text_edit_singleline(name);
                                });
                                if ui.button("save").clicked() {
                                    let _ = channels.back_end_notifier.blocking_send(to_data(MessageToPackageBack::SavePack(name.clone(), pack.clone())));
                                    *status = ImportStatus::WaitingForSave;
                                }
                            }
                        }
                        ImportStatus::WaitingForSave => {
                            ui.colored_label(egui::Color32::GREEN, "Waiting for pack to be saved.");
                        }
                        ImportStatus::PackError(e) => {
                            let error_msg = format!("failed to import pack due to error: {e:#?}");
                            if ui.button("clear").on_hover_text(
                                "This will cancel any pack import in progress. If import is already finished, then it wil simply clear the import status").clicked() {
                                    *status = ImportStatus::UnInitialized;
                            }
                            ui.colored_label(
                                egui::Color32::RED,
                                error_msg,
                            );
                        }
                    }
                }
            }

            Ok(())
        });
    }

    pub fn save(&mut self, mut texture_pack: LoadedPackTexture, report: PackageImportReport) {
        /*
            We save in a file with the name of the package, while we keep track of it from a uuid point of view.
            It means we can have duplicates unless package with same name is deleted.
        */
        let mut to_delete: Vec<Uuid> = Vec::new();
        for (uuid, pack) in self.packs.iter() {
            if pack.name == texture_pack.name {
                to_delete.push(*uuid);
            }
        }
        self.delete_packs(to_delete);
        self.tasks.save_texture(&mut texture_pack, true);
        self.packs.insert(texture_pack.uuid, texture_pack);
        self.reports.insert(report.uuid, report);
    }
}

impl Component for PackageUIManager {
    fn init(&mut self) {}

    fn flush_all_messages(&mut self) {
        assert!(self.channels.is_some());
        let channels = self.channels.as_mut().unwrap();

        if let Ok(mut import_status) = self.state.import_status.lock() {
            if let ImportStatus::LoadingPack(file_path) = &mut *import_status {
                let _ = channels
                    .back_end_notifier
                    .blocking_send(to_data(MessageToPackageBack::ImportPack(file_path.clone())));
                *import_status = ImportStatus::WaitingLoading(file_path.clone());
            }
        }
        let mut messages = Vec::new();
        while let Ok(msg) = channels.back_end_receiver.try_recv() {
            messages.push(from_data(&msg));
        }
        for msg in messages {
            self.handle_message(msg);
        }
    }

    fn tick(&mut self, timestamp: f64) -> ComponentResult {
        assert!(self.channels.is_some());

        let raw_link = {
            let channels = self.channels.as_mut().unwrap();
            //trace!("blocking waiting for subscription_mumblelink {}", channels.subscription_mumblelink.len());
            channels.subscription_mumblelink.try_recv().unwrap()
        };
        let link_result: MumbleLinkResult = from_broadcast(&raw_link);
        //trace!("subscription_mumblelink provided data");

        for (pack_uuid, tex_path, marker_uuid, position, common_attributes) in
            std::mem::take(&mut self.delayed_marker_texture)
        {
            self.load_marker_texture(
                pack_uuid,
                &Arc::clone(&self.egui_context),
                tex_path,
                marker_uuid,
                position,
                common_attributes,
            );
        }
        for (pack_uuid, tex_path, trail_uuid, common_attributes) in
            std::mem::take(&mut self.delayed_trail_texture)
        {
            self.load_trail_texture(
                pack_uuid,
                &Arc::clone(&self.egui_context),
                tex_path,
                trail_uuid,
                common_attributes,
            );
        }

        //let channels = self.channels.as_mut().unwrap();
        //let raw_z_near = channels.subscription_near_scene.blocking_recv().unwrap();
        //let z_near: f32 = from_data(raw_z_near);
        if let Some(link) = link_result.link.as_ref() {
            let _ = self._tick(timestamp, link, self.z_near);
        }
        to_broadcast(self.state.clone())
    }
    fn bind(&mut self, mut channels: ComponentChannels) {
        let (back_end_notifier, back_end_receiver) = channels.peers.remove(&0).unwrap();
        let channels = PackageUIChannels {
            subscription_mumblelink: channels.requirements.remove(&1).unwrap(),
            back_end_notifier,
            back_end_receiver,
            renderer_notifier: channels.notify.remove(&2).unwrap(),
        };

        self.channels = Some(channels);
    }
    fn notify(&self) -> Vec<&str> {
        vec!["ui:jokolay_renderer"]
    }
    fn peers(&self) -> Vec<&str> {
        vec!["back:jokolay_package_manager"]
    }
    fn requirements(&self) -> Vec<&str> {
        vec!["ui:mumble_link"]
    }
    fn accept_notifications(&self) -> bool {
        false
    }
}

impl UIPanel for PackageUIManager {
    fn areas(&self) -> Vec<UIArea> {
        vec![
            UIArea {
                is_open: false,
                name: "Package Manager".to_string(),
                id: "package_loading".to_string(),
            },
            UIArea {
                is_open: false,
                name: "File Manager".to_string(),
                id: "file_manager".to_string(),
            },
        ]
    }
    fn init(&mut self) {}
    fn gui(&mut self, is_open: &mut bool, area_id: &str) {
        match area_id {
            "package_loading" => {
                self.gui_package_list(is_open);
            }
            "file_manager" => {
                self.gui_file_manager(is_open);
            }
            _ => {}
        }
    }
    fn menu_ui(&mut self, ui: &mut egui::Ui) {
        let nb_running_tasks_on_back: i32 = 0;
        let nb_running_tasks_on_network: i32 = 0;
        ui.menu_button("Markers", |ui| {
            if self.show_only_active {
                if ui.button("Show everything").clicked() {
                    self.show_only_active = false;
                }
            } else if ui.button("Show only active").clicked() {
                self.show_only_active = true;
            }
            if ui.button("Activate all elements").clicked() {
                self.category_set_all(true);
                let channels = self.channels.as_mut().unwrap();
                let _ = channels
                    .back_end_notifier
                    .blocking_send(to_data(MessageToPackageBack::CategorySetAll(true)));
            }
            if ui.button("Deactivate all elements").clicked() {
                self.category_set_all(false);
                let channels = self.channels.as_mut().unwrap();
                let _ = channels
                    .back_end_notifier
                    .blocking_send(to_data(MessageToPackageBack::CategorySetAll(false)));
            }

            let channels = self.channels.as_mut().unwrap();
            for (pack, import_quality_report) in
                std::iter::zip(self.packs.values_mut(), self.reports.values())
            {
                //pack.is_dirty = pack.is_dirty || force_activation || force_deactivation;
                //category_sub_menu is for display only, it's a bad idea to use it to manipulate status
                pack.category_sub_menu(
                    &channels.back_end_notifier,
                    ui,
                    self.show_only_active,
                    import_quality_report,
                );
            }
        });
        if self.tasks.is_running()
            || nb_running_tasks_on_back > 0
            || nb_running_tasks_on_network > 0
        {
            let sp = egui::Spinner::new()
                .color(self.status_as_color(nb_running_tasks_on_back, nb_running_tasks_on_network));
            ui.add(sp);
        }
    }
}
