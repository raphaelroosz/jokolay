use std::{
    collections::BTreeMap,
    io::Write,
    ops::DerefMut,
    sync::{Arc, Mutex},
    thread,
};

use cap_std::fs_utf8::Dir;
use egui_window_glfw_passthrough::{glfw::Context as _, GlfwBackend, GlfwConfig};
mod init;
mod mumble;
mod ui_parameters;
use init::{get_jokolay_dir, get_jokolay_path};
use jmf::{
    message::{UIToBackMessage, UIToUIMessage},
    PackageDataManager, PackageUIManager,
};
use uuid::Uuid;
//use jmf::FileManager;
use crate::app::mumble::mumble_gui;
use crate::manager::{theme::ThemeManager, trace::JokolayTracingLayer};

use jmf::message::BackToUIMessage;
use jmf::{
    build_from_core, jokolay_to_editable_path, jokolay_to_extract_path, LoadedPackData,
    LoadedPackTexture,
};
use jmf::{import_pack_from_zip_file_path, ImportStatus};
use joko_render::renderer::JokoRenderer;
use jokolink::{MumbleChanges, MumbleLink, MumbleManager};
use miette::{Context, IntoDiagnostic, Result};
use tracing::{error, info, info_span};

const MINIMAL_WINDOW_WIDTH: u32 = 640;
const MINIMAL_WINDOW_HEIGHT: u32 = 480;
const MINIMAL_WINDOW_POSITION_X: i32 = 0;
const MINIMAL_WINDOW_POSITION_Y: i32 = 0;

struct JokolayUIState {
    link: Option<MumbleLink>,
    editable_mumble: bool,
    window_changed: bool,
    list_of_textures_changed: bool, //Meant as an optimisation to only update when choice_of_category_changed have produced the list of textures to display
    first_load_done: bool,
    nb_running_tasks_on_back: i32, // store the number of running tasks in background thread
    nb_running_tasks_on_network: i32, // store the number of running tasks (requests) in progress
    import_status: Arc<Mutex<ImportStatus>>,
    maximal_window_width: u32,
    maximal_window_height: u32,
    root_path: std::path::PathBuf,
}

struct JokolayBackState {
    choice_of_category_changed: bool, //Meant as an optimisation to only update when there is a change in UI
    read_ui_link: bool,
    copy_of_ui_link: Option<MumbleLink>,
    root_dir: Arc<Dir>,
    editable_path: std::path::PathBuf,
    extract_path: std::path::PathBuf,
}
struct JokolayApp {
    mumble_manager: MumbleManager,
    package_manager: PackageDataManager,
}
struct JokolayGui {
    ui_configuration: ui_parameters::JokolayUIConfiguration,
    menu_panel: MenuPanel,
    joko_renderer: JokoRenderer,
    egui_context: egui::Context,
    glfw_backend: GlfwBackend,
    theme_manager: ThemeManager,
    mumble_manager: MumbleManager,
    package_manager: PackageUIManager,
}
#[allow(unused)]
pub struct Jokolay {
    gui: Arc<Mutex<JokolayGui>>,
    app: Arc<Mutex<Box<JokolayApp>>>,
    state_ui: JokolayUIState,
    state_back: JokolayBackState,
}

impl Jokolay {
    pub fn new(root_dir: Arc<Dir>, root_path: std::path::PathBuf) -> Result<Self> {
        /*
            We have two mumble_managers, one for UI, one for backend, each keeping its own copy
            this avoid transmition between threads to read same data from system
            It happens anyway when the UI start the edit mode of the mumble link.
        */

        let mumble_data_manager =
            MumbleManager::new("MumbleLink", None).wrap_err("failed to create mumble manager")?;
        let mumble_ui_manager =
            MumbleManager::new("MumbleLink", None).wrap_err("failed to create mumble manager")?;

        let data_packages: BTreeMap<Uuid, LoadedPackData> = Default::default();
        let texture_packages: BTreeMap<Uuid, LoadedPackTexture> = Default::default();
        let package_data_manager = PackageDataManager::new(data_packages, Arc::clone(&root_dir))?;
        let mut package_ui_manager = PackageUIManager::new(texture_packages);
        let mut theme_manager =
            ThemeManager::new(Arc::clone(&root_dir)).wrap_err("failed to create theme manager")?;

        let egui_context = egui::Context::default();
        theme_manager.init_egui(&egui_context);
        let mut glfw_backend = GlfwBackend::new(GlfwConfig {
            glfw_callback: Box::new(|glfw_context| {
                glfw_context.window_hint(
                    egui_window_glfw_passthrough::glfw::WindowHint::SRgbCapable(true),
                );
                glfw_context.window_hint(egui_window_glfw_passthrough::glfw::WindowHint::Floating(
                    true,
                ));
                glfw_context.window_hint(
                    egui_window_glfw_passthrough::glfw::WindowHint::ContextVersion(4, 6),
                );
            }),
            opengl_window: Some(true),
            transparent_window: Some(true),
            window_title: "Jokolay".to_string(),
            ..Default::default()
        });

        //retrieve current screen resolution
        let video_mode = glfw_backend.glfw.with_primary_monitor(|_, m| {
            if let Some(m) = m {
                m.get_video_mode()
            } else {
                None
            }
        });

        glfw_backend.window.set_floating(true);
        glfw_backend.window.set_decorated(false);
        let joko_renderer = JokoRenderer::new(&mut glfw_backend, Default::default());

        //TODO: load configuration from disk (ui.toml)
        let editable_path = jokolay_to_editable_path(&root_path)
            .to_str()
            .unwrap()
            .to_string();
        let ui_configuration = ui_parameters::JokolayUIConfiguration::new(
            glfw_backend.glfw.get_time() as _,
            editable_path.clone(),
        );

        let menu_panel = MenuPanel::default();

        package_ui_manager.late_init(&egui_context);
        Ok(Self {
            gui: Arc::new(Mutex::new(JokolayGui {
                ui_configuration,
                joko_renderer,
                glfw_backend,
                egui_context,
                menu_panel,
                theme_manager,
                mumble_manager: mumble_ui_manager,
                package_manager: package_ui_manager,
            })),
            app: Arc::new(Mutex::new(Box::new(JokolayApp {
                mumble_manager: mumble_data_manager,
                package_manager: package_data_manager,
            }))),
            state_ui: JokolayUIState {
                link: Some(MumbleLink::default()),
                editable_mumble: false,
                window_changed: true,
                list_of_textures_changed: false,
                first_load_done: false,
                nb_running_tasks_on_back: 0,
                nb_running_tasks_on_network: 0,
                import_status: Default::default(),
                maximal_window_width: video_mode.unwrap().width, //TODO: what happens if change of screen ?
                maximal_window_height: video_mode.unwrap().height,
                root_path: root_path.clone(),
            },
            state_back: JokolayBackState {
                choice_of_category_changed: false,
                read_ui_link: false,
                copy_of_ui_link: Default::default(),
                root_dir,
                editable_path: std::path::PathBuf::from(editable_path),
                extract_path: std::path::PathBuf::from(jokolay_to_extract_path(&root_path)),
            },
        })
    }

    fn start_background_loop(
        app: Arc<Mutex<Box<JokolayApp>>>,
        state: JokolayBackState,
        b2u_sender: std::sync::mpsc::Sender<BackToUIMessage>,
        u2b_receiver: std::sync::mpsc::Receiver<UIToBackMessage>,
    ) {
        let _background_thread = std::thread::spawn(move || {
            // Load the directory with packages in the background process
            {
                //TODO: lazy loading to load maps only when on it
                let mut app = app.lock().unwrap();
                let JokolayApp {
                    mumble_manager: _,
                    package_manager,
                } = &mut app.deref_mut().as_mut();
                package_manager.load_all(Arc::clone(&state.root_dir), &b2u_sender);
            }
            Self::background_loop(Arc::clone(&app), state, b2u_sender, u2b_receiver);
        });
    }

    fn handle_u2b_message(
        package_manager: &mut PackageDataManager,
        local_state: &mut JokolayBackState,
        b2u_sender: &std::sync::mpsc::Sender<BackToUIMessage>,
        msg: UIToBackMessage,
    ) {
        match msg {
            UIToBackMessage::ActiveFiles(currently_used_files) => {
                tracing::trace!("Handling of UIToBackMessage::ActiveFiles");
                package_manager.set_currently_used_files(currently_used_files);
                local_state.choice_of_category_changed = true;
            }
            UIToBackMessage::CategoryActivationElementStatusChange(category_uuid, status) => {
                tracing::trace!(
                    "Handling of UIToBackMessage::CategoryActivationElementStatusChange"
                );
                package_manager.category_set(category_uuid, status);
            }
            UIToBackMessage::CategoryActivationBranchStatusChange(category_uuid, status) => {
                tracing::trace!(
                    "Handling of UIToBackMessage::CategoryActivationBranchStatusChange"
                );
                package_manager.category_branch_set(category_uuid, status);
            }
            UIToBackMessage::CategoryActivationStatusChanged => {
                tracing::trace!("Handling of UIToBackMessage::CategoryActivationStatusChanged");
                local_state.choice_of_category_changed = true;
            }
            UIToBackMessage::CategorySetAll(status) => {
                tracing::trace!("Handling of UIToBackMessage::CategorySetAll");
                package_manager.category_set_all(status);
                local_state.choice_of_category_changed = true;
            }
            UIToBackMessage::DeletePacks(to_delete) => {
                tracing::trace!("Handling of UIToBackMessage::DeletePacks");
                let mut deleted = Vec::new();
                for pack_uuid in to_delete {
                    if let Some(pack) = package_manager.packs.remove(&pack_uuid) {
                        if let Err(e) = package_manager.marker_packs_dir.remove_dir_all(&pack.name)
                        {
                            error!(?e, pack.name, "failed to remove pack");
                        } else {
                            info!("deleted marker pack: {}", pack.name);
                            deleted.push(pack_uuid);
                        }
                    }
                }
                let _ = b2u_sender.send(BackToUIMessage::DeletedPacks(deleted));
            }
            UIToBackMessage::ImportPack(file_path) => {
                tracing::trace!("Handling of UIToBackMessage::ImportPack");
                let _ = b2u_sender.send(BackToUIMessage::NbTasksRunning(1));
                let start = std::time::SystemTime::now();
                let result = import_pack_from_zip_file_path(file_path, &local_state.extract_path);
                let elaspsed = start.elapsed().unwrap_or_default();
                tracing::info!(
                    "Loading of taco package from disk took {} ms",
                    elaspsed.as_millis()
                );
                match result {
                    Ok((file_name, pack)) => {
                        let _ = b2u_sender.send(BackToUIMessage::ImportedPack(file_name, pack));
                    }
                    Err(e) => {
                        let _ = b2u_sender.send(BackToUIMessage::ImportFailure(e));
                    }
                }
                let _ = b2u_sender.send(BackToUIMessage::NbTasksRunning(0));
            }
            UIToBackMessage::MumbleLinkAutonomous => {
                tracing::trace!("Handling of UIToBackMessage::MumbleLinkAutonomous");
                local_state.read_ui_link = false;
            }
            UIToBackMessage::MumbleLinkBindedOnUI => {
                tracing::trace!("Handling of UIToBackMessage::MumbleLinkBindedOnUI");
                local_state.read_ui_link = true;
            }
            UIToBackMessage::MumbleLink(link) => {
                tracing::trace!("Handling of UIToBackMessage::MumbleLink");
                local_state.copy_of_ui_link = link;
            }
            UIToBackMessage::ReloadPack => {
                unimplemented!(
                    "Handling of UIToBackMessage::ReloadPack has not been implemented yet"
                );
            }
            UIToBackMessage::SavePack(name, pack) => {
                tracing::trace!("Handling of UIToBackMessage::SavePack");
                let name = name.as_str();
                if package_manager.marker_packs_dir.exists(name) {
                    match package_manager
                        .marker_packs_dir
                        .remove_dir_all(name)
                        .into_diagnostic()
                    {
                        Ok(_) => {}
                        Err(e) => {
                            error!(?e, "failed to delete already existing marker pack");
                        }
                    }
                }
                if let Err(e) = package_manager.marker_packs_dir.create_dir_all(name) {
                    error!(?e, "failed to create directory for pack");
                }
                match package_manager.marker_packs_dir.open_dir(name) {
                    Ok(dir) => {
                        let (data_pack, mut texture_pack, mut report) =
                            build_from_core(name.to_string(), dir.into(), pack);
                        tracing::trace!("Package loaded into data and texture");
                        let uuid_of_insertion = package_manager.save(data_pack, report.clone());
                        report.uuid = uuid_of_insertion;
                        texture_pack.uuid = uuid_of_insertion;
                        let _ = b2u_sender.send(BackToUIMessage::LoadedPack(texture_pack, report));
                    }
                    Err(e) => {
                        error!(
                            ?e,
                            "failed to open marker pack directory to save pack {:?} {}",
                            package_manager.marker_packs_dir,
                            name
                        );
                    }
                };
            }
            UIToBackMessage::SaveUIConfiguration(serialized_string) => {
                //let _ = b2u_sender.send(BackToUIMessage::NbTasksRunning(package_manager.tasks.count()+ 1)); //TODO: send update on screen
                match local_state
                    .root_dir
                    .create(ui_parameters::UI_PARAMETERS_FILE_NAME)
                {
                    Ok(mut file) => {
                        match file.write(serialized_string.as_bytes()).into_diagnostic() {
                            Ok(_) => {}
                            Err(e) => {
                                error!(?e, "failed to save UI configuration");
                            }
                        }
                    }
                    Err(e) => {
                        error!(?e, "failed to open UI configuration file");
                    }
                }
            }
            _ => {
                unimplemented!("Handling BackToUIMessage has not been implemented yet");
            }
        }
    }
    fn background_loop(
        app: Arc<Mutex<Box<JokolayApp>>>,
        mut local_state: JokolayBackState,
        b2u_sender: std::sync::mpsc::Sender<BackToUIMessage>,
        u2b_receiver: std::sync::mpsc::Receiver<UIToBackMessage>,
    ) {
        tracing::info!("entering background event loop");
        let span_guard = info_span!("background event loop").entered();
        let mut loop_index: u128 = 0;
        let mut nb_messages: u128 = 0;
        loop {
            tracing::trace!("background loop tick: {} {}", loop_index, nb_messages);
            let mut app = app.lock().unwrap();
            let JokolayApp {
                mumble_manager,
                package_manager,
            } = &mut app.deref_mut().as_mut();

            while let Ok(msg) = u2b_receiver.try_recv() {
                Self::handle_u2b_message(package_manager, &mut local_state, &b2u_sender, msg);
                nb_messages += 1;
            }
            let link = if local_state.read_ui_link {
                local_state.copy_of_ui_link.as_ref()
            } else {
                match mumble_manager.tick() {
                    Ok(ml) => ml,
                    Err(e) => {
                        error!(?e, "mumble manager tick error");
                        None
                    }
                }
            };
            tracing::trace!(
                "choice_of_category_changed: {}",
                local_state.choice_of_category_changed
            );
            package_manager.tick(
                &b2u_sender,
                loop_index,
                link,
                local_state.choice_of_category_changed,
            );
            local_state.choice_of_category_changed = false;

            thread::sleep(std::time::Duration::from_millis(10));
            loop_index += 1;
        }
        unreachable!("Program broke out a never ending loop !");
        drop(span_guard);
    }

    fn handle_u2u_message(gui: &mut JokolayGui, msg: UIToUIMessage) {
        match msg {
            UIToUIMessage::BulkMarkerObject(marker_objects) => {
                tracing::debug!(
                    "Handling of UIToUIMessage::BulkMarkerObject {}",
                    marker_objects.len()
                );
                gui.joko_renderer.extend_markers(marker_objects);
            }
            UIToUIMessage::BulkTrailObject(trail_objects) => {
                tracing::debug!(
                    "Handling of UIToUIMessage::BulkTrailObject {}",
                    trail_objects.len()
                );
                gui.joko_renderer.extend_trails(trail_objects);
            }
            UIToUIMessage::MarkerObject(mo) => {
                tracing::trace!("Handling of UIToUIMessage::MarkerObject");
                gui.joko_renderer.add_billboard(mo);
            }
            UIToUIMessage::TrailObject(to) => {
                tracing::trace!("Handling of UIToUIMessage::TrailObject");
                gui.joko_renderer.add_trail(to);
            }
            UIToUIMessage::RenderSwapChain => {
                tracing::debug!("Handling of UIToUIMessage::RenderSwapChain");
                gui.joko_renderer.swap();
            }
            _ => {
                unimplemented!("Handling UIToUIMessage has not been implemented yet");
            }
        }
    }
    fn handle_b2u_message(
        gui: &mut JokolayGui,
        local_state: &mut JokolayUIState,
        u2b_sender: &std::sync::mpsc::Sender<UIToBackMessage>,
        msg: BackToUIMessage,
    ) {
        match msg {
            BackToUIMessage::ActiveElements(active_elements) => {
                tracing::trace!("Handling of BackToUIMessage::ActiveElements");
                gui.package_manager
                    .update_active_categories(&active_elements);
            }
            BackToUIMessage::CurrentlyUsedFiles(currently_used_files) => {
                tracing::trace!("Handling of BackToUIMessage::CurrentlyUsedFiles");
                gui.package_manager
                    .set_currently_used_files(currently_used_files);
            }
            BackToUIMessage::DeletedPacks(to_delete) => {
                tracing::trace!("Handling of BackToUIMessage::DeletedPacks");
                gui.package_manager.delete_packs(to_delete);
            }
            BackToUIMessage::FirstLoadDone => {
                local_state.first_load_done = true;
            }
            BackToUIMessage::ImportedPack(file_name, pack) => {
                tracing::trace!("Handling of BackToUIMessage::ImportedPack");
                *local_state.import_status.lock().unwrap() =
                    ImportStatus::PackDone(file_name, pack, false);
            }
            BackToUIMessage::ImportFailure(error) => {
                tracing::trace!("Handling of BackToUIMessage::ImportFailure");
                *local_state.import_status.lock().unwrap() = ImportStatus::PackError(error);
            }
            BackToUIMessage::LoadedPack(pack_texture, report) => {
                tracing::trace!("Handling of BackToUIMessage::LoadedPack");
                gui.package_manager.save(pack_texture, report);
                local_state.import_status = Default::default();
                let _ = u2b_sender.send(UIToBackMessage::CategoryActivationStatusChanged);
            }
            BackToUIMessage::MarkerTexture(
                pack_uuid,
                tex_path,
                marker_uuid,
                position,
                common_attributes,
            ) => {
                tracing::trace!("Handling of BackToUIMessage::MarkerTexture");
                gui.package_manager.load_marker_texture(
                    &gui.egui_context,
                    pack_uuid,
                    tex_path,
                    marker_uuid,
                    position,
                    common_attributes,
                );
            }
            BackToUIMessage::NbTasksRunning(nb_tasks) => {
                tracing::trace!("Handling of BackToUIMessage::NbTasksRunning");
                local_state.nb_running_tasks_on_back = nb_tasks;
            }
            BackToUIMessage::PackageActiveElements(pack_uuid, active_elements) => {
                tracing::trace!("Handling of BackToUIMessage::PackageActiveElements");
                gui.package_manager
                    .update_pack_active_categories(pack_uuid, &active_elements);
            }
            BackToUIMessage::TextureSwapChain => {
                tracing::debug!("Handling of BackToUIMessage::TextureSwapChain");
                gui.package_manager.swap();
                local_state.list_of_textures_changed = true;
            }
            BackToUIMessage::TrailTexture(pack_uuid, tex_path, trail_uuid, common_attributes) => {
                tracing::trace!("Handling of BackToUIMessage::TrailTexture");
                gui.package_manager.load_trail_texture(
                    &gui.egui_context,
                    pack_uuid,
                    tex_path,
                    trail_uuid,
                    common_attributes,
                );
            }
            _ => {
                unimplemented!("Handling BackToUIMessage has not been implemented yet");
            }
        }
    }

    pub fn enter_event_loop(self) {
        let (b2u_sender, b2u_receiver) = std::sync::mpsc::channel();
        let (u2b_sender, u2b_receiver) = std::sync::mpsc::channel();
        let (u2u_sender, u2u_receiver) = std::sync::mpsc::channel();
        Self::start_background_loop(
            Arc::clone(&self.app),
            self.state_back,
            b2u_sender,
            u2b_receiver,
        );

        tracing::info!("entering glfw event loop");
        let span_guard = info_span!("glfw event loop").entered();
        let mut local_state = self.state_ui;
        let mut nb_frames: u128 = 0;
        let mut nb_messages: u128 = 0;
        let max_nb_messages_per_loop: u128 = 100;
        //u2u_sender.send(UIToUIMessage::Present);// force a first drawing
        loop {
            {
                let mut nb_message_on_curent_loop: u128 = 0;
                tracing::trace!(
                    "glfw event loop, {} frames, {} messages",
                    nb_frames,
                    nb_messages
                );

                if let Ok(mut import_status) = local_state.import_status.lock() {
                    match &mut *import_status {
                        ImportStatus::LoadingPack(file_path) => {
                            let _ = u2b_sender.send(UIToBackMessage::ImportPack(file_path.clone()));
                            *import_status = ImportStatus::WaitingLoading(file_path.clone());
                        }
                        _ => {}
                    }
                }
                //untested and might crash due to .unwrap()
                let mut gui = self.gui.lock().unwrap();
                while let Ok(msg) = u2u_receiver.try_recv() {
                    nb_messages += 1;
                    Self::handle_u2u_message(gui.deref_mut(), msg);
                    nb_message_on_curent_loop += 1;
                    if nb_message_on_curent_loop == max_nb_messages_per_loop {
                        break;
                    }
                }
                if nb_message_on_curent_loop < max_nb_messages_per_loop {
                    while let Ok(msg) = b2u_receiver.try_recv() {
                        nb_messages += 1;
                        Self::handle_b2u_message(
                            gui.deref_mut(),
                            &mut local_state,
                            &u2b_sender,
                            msg,
                        );
                        nb_message_on_curent_loop += 1;
                        if nb_message_on_curent_loop == max_nb_messages_per_loop {
                            break;
                        }
                    }
                }
            }

            let mut gui = self.gui.lock().unwrap();
            let JokolayGui {
                ui_configuration,
                menu_panel,
                joko_renderer,
                egui_context,
                glfw_backend,
                theme_manager,
                mumble_manager,
                package_manager,
            } = &mut gui.deref_mut();
            let latest_time = glfw_backend.glfw.get_time();

            let etx = egui_context.clone();

            /*
            if etx.input(|i| {
                TODO:
                    handle shortcuts
                    a module publish a list of shortcuts
                    At import, user need to accept those.
                    We can't have a module that is a keyboard listener.

                    modifiers are not forwarded.
                println!("{:?} {:?}", i.keys_down, i.modifiers);
                false
            }) {
            }
            */

            // gather events
            glfw_backend.glfw.poll_events();
            glfw_backend.tick();

            if glfw_backend.window.should_close() {
                tracing::warn!("should close is true. So, exiting event loop");
                break;
            }

            if glfw_backend.resized_event_pending {
                let latest_size = glfw_backend.window.get_framebuffer_size();
                let latest_size = [latest_size.0 as _, latest_size.1 as _];

                glfw_backend.framebuffer_size_physical = latest_size;
                glfw_backend.window_size_logical = [
                    latest_size[0] as f32 / glfw_backend.scale,
                    latest_size[1] as f32 / glfw_backend.scale,
                ];
                joko_renderer.resize_framebuffer(latest_size);
                glfw_backend.resized_event_pending = false;
            }
            joko_renderer.prepare_frame(|| {
                let latest_size = glfw_backend.window.get_framebuffer_size();
                tracing::info!(
                    ?latest_size,
                    "failed to get surface texture, so calling latest framebuffer size"
                );
                let latest_size = [latest_size.0 as _, latest_size.1 as _];
                glfw_backend.framebuffer_size_physical = latest_size;
                glfw_backend.window_size_logical = [
                    latest_size[0] as f32 / glfw_backend.scale,
                    latest_size[1] as f32 / glfw_backend.scale,
                ];
                latest_size
            });

            let mut input = glfw_backend.take_raw_input();
            input.time = Some(latest_time);

            etx.begin_frame(input);

            // do all the non-gui stuff first
            ui_configuration.tick(latest_time);
            if local_state.editable_mumble {
                local_state.window_changed = true;
                local_state.link.as_mut().unwrap().changes = enumflags2::BitFlags::all();
                let _ = u2b_sender.send(UIToBackMessage::MumbleLink(local_state.link.clone()));
            } else {
                let is_mumble_alive = mumble_manager.is_alive();
                match mumble_manager.tick() {
                    Ok(ml) => {
                        if let Some(link) = ml {
                            if link.changes.contains(MumbleChanges::WindowPosition)
                                || link.changes.contains(MumbleChanges::WindowSize)
                            {
                                local_state.window_changed = true;
                            }
                            if is_mumble_alive {
                                local_state.link = Some(link.clone());
                            }
                        }
                    }
                    Err(e) => {
                        error!(?e, "mumble manager tick error");
                    }
                }
            }

            // check if we need to change window position or size.
            if let Some(link) = local_state.link.as_ref() {
                if local_state.window_changed {
                    glfw_backend.window.set_pos(
                        link.client_pos.x.max(MINIMAL_WINDOW_POSITION_X),
                        link.client_pos.y.max(MINIMAL_WINDOW_POSITION_Y),
                    );
                    // if gw2 is in windowed fullscreen mode, then the size is full resolution of the screen/monitor.
                    // But if we set that size, when you focus jokolay, the screen goes blank on win11 (some kind of fullscreen optimization maybe?)
                    // so we remove a pixel from right/bottom edges. mostly indistinguishable, but makes sure that transparency works even in windowed fullscrene mode of gw2
                    let client_size_x = MINIMAL_WINDOW_WIDTH
                        .max(link.client_size.x)
                        .min(local_state.maximal_window_width);
                    let client_size_y = MINIMAL_WINDOW_HEIGHT
                        .max(link.client_size.y)
                        .min(local_state.maximal_window_height);
                    glfw_backend
                        .window
                        .set_size((client_size_x - 1) as i32, (client_size_y - 1) as i32);
                }
                if local_state.list_of_textures_changed
                    || link.changes.contains(MumbleChanges::Position)
                    || link.changes.contains(MumbleChanges::Map)
                {
                    package_manager.tick(
                        &u2u_sender,
                        latest_time,
                        link,
                        JokoRenderer::get_z_near(),
                    );
                    local_state.list_of_textures_changed = false;
                }
                local_state.window_changed = false;
            }

            joko_renderer.tick(local_state.link.as_ref());
            menu_panel.tick(&etx, local_state.link.as_ref());

            // do the gui stuff now
            egui::Area::new("menu panel")
                .fixed_pos(menu_panel.pos)
                .interactable(true)
                .order(egui::Order::Foreground)
                .show(&etx, |ui| {
                    ui.style_mut().visuals.widgets.inactive.weak_bg_fill =
                        egui::Color32::TRANSPARENT;
                    ui.horizontal(|ui| {
                        //TODO: if any displayed, show an additional "hide all"
                        ui.menu_button(
                            egui::RichText::new("JKL")
                                .size((MenuPanel::HEIGHT - 2.0) * menu_panel.ui_scaling_factor)
                                .background_color(egui::Color32::TRANSPARENT),
                            |ui| {
                                ui.checkbox(
                                    &mut menu_panel.show_parameters_manager,
                                    "Configuration",
                                );
                                ui.checkbox(&mut menu_panel.show_theme_window, "Themes");
                                ui.checkbox(
                                    &mut menu_panel.show_package_manager_window,
                                    "Package Manager",
                                );
                                ui.checkbox(
                                    &mut menu_panel.show_mumble_manager_window,
                                    "Mumble Manager",
                                );
                                ui.checkbox(
                                    &mut menu_panel.show_file_manager_window,
                                    "File Manager",
                                );
                                //ui.checkbox(&mut menu_panel.show_tracing_window, "Show Logs");
                                if menu_panel.show_parameters_manager
                                    || menu_panel.show_package_manager_window
                                    || menu_panel.show_mumble_manager_window
                                    || menu_panel.show_theme_window
                                    || menu_panel.show_file_manager_window
                                    || menu_panel.show_tracing_window
                                {
                                    if ui.button("Close all panels").clicked() {
                                        menu_panel.show_parameters_manager = false;
                                        menu_panel.show_package_manager_window = false;
                                        menu_panel.show_mumble_manager_window = false;
                                        menu_panel.show_theme_window = false;
                                        menu_panel.show_file_manager_window = false;
                                        menu_panel.show_tracing_window = false;
                                    }
                                }
                                if ui.button("exit").clicked() {
                                    info!("exiting jokolay");
                                    glfw_backend.window.set_should_close(true);
                                }
                            },
                        );
                        package_manager.menu_ui(
                            &u2b_sender,
                            &u2u_sender,
                            ui,
                            local_state.nb_running_tasks_on_back,
                            local_state.nb_running_tasks_on_network,
                        );
                    });
                });

            if let Some(link) = local_state.link.as_mut() {
                mumble_gui(
                    &u2b_sender,
                    &etx,
                    &mut menu_panel.show_mumble_manager_window,
                    &mut local_state.editable_mumble,
                    link,
                );
            };
            package_manager.gui(
                &u2b_sender,
                &etx,
                &mut menu_panel.show_package_manager_window,
                &local_state.import_status,
                &mut menu_panel.show_file_manager_window,
                local_state.first_load_done,
            );
            JokolayTracingLayer::gui(&etx, &mut menu_panel.show_tracing_window);
            theme_manager.gui(&etx, &mut menu_panel.show_theme_window);
            ui_configuration.gui(
                &u2b_sender,
                &etx,
                glfw_backend,
                &mut menu_panel.show_parameters_manager,
                &local_state.root_path,
            );
            // show notifications
            JokolayTracingLayer::show_notifications(&etx);

            // end gui stuff
            etx.request_repaint();

            let egui::FullOutput {
                platform_output,
                textures_delta,
                shapes,
                ..
            } = etx.end_frame();

            if !platform_output.copied_text.is_empty() {
                glfw_backend
                    .window
                    .set_clipboard_string(&platform_output.copied_text);
            }

            // if it doesn't require either keyboard or pointer, set passthrough to true
            glfw_backend
                .window
                .set_mouse_passthrough(!(etx.wants_keyboard_input() || etx.wants_pointer_input()));
            //TODO: view from above when map is open
            /*
            TODO: have a clean view when game is not focused.
            let mut do_draw = local_state.editable_mumble;
            if !do_draw {
                if let Some(link) = local_state.link.as_ref() {
                    if let Some(ui_state) = link.ui_state {
                        do_draw = ui_state.contains(UIState::GameHasFocus)
                    }
                };
            }*/

            let animation_time = if ui_configuration.display_parameters.animate {
                latest_time
            } else {
                0.0
            };

            joko_renderer.render_egui(
                etx.tessellate(shapes, etx.pixels_per_point()),
                textures_delta,
                glfw_backend.window_size_logical,
                animation_time,
            );
            joko_renderer.present();
            glfw_backend.window.swap_buffers();

            nb_frames += 1;
        }
        drop(span_guard);
    }
}

pub fn start_jokolay() {
    let jokolay_dir = match get_jokolay_dir() {
        Ok(jdir) => jdir,
        Err(e) => {
            eprintln!("failed to create jokolay dir: {e:#?}");
            panic!("failed to create jokolay_dir: {e:#?}");
        }
    };
    let jokolay_path = get_jokolay_path().unwrap();

    let log_file_flush_guard = match JokolayTracingLayer::install_tracing(&jokolay_dir) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("failed to install tracing: {e:#?}");
            panic!("failed to install tracing: {e:#?}");
        }
    };

    if let Err(e) = rayon::ThreadPoolBuilder::default()
        .panic_handler(|panic_info| {
            error!(?panic_info, "rayon thread paniced.");
        })
        .build_global()
    {
        error!(
            ?e,
            "failed to set panic handler and build global threadpool for rayon"
        );
    }

    match Jokolay::new(jokolay_dir.into(), jokolay_path) {
        Ok(jokolay) => {
            jokolay.enter_event_loop();
        }
        Err(e) => {
            error!(?e, "failed to create Jokolay App");
        }
    };
    std::mem::drop(log_file_flush_guard);
}

/// Guild Wars 2 has an array of menu icons on top left corner of the game.
/// Its size is affected by four different factors
/// 1. UISZ:
///     This is a setting in graphics options of gw2 and it comes in 4 variants
///     small, normal, large and larger.
///     This is something we can get from mumblelink's context.
/// 2. DPI scaling
///     This is a setting in graphics options too. When scaling is enabled, sizes of menu become bigger according to the dpi of gw2 window
///     This is something we get from gw2's config file in AppData/Roaming and store in mumble link as dpi scaling
///     We also get dpi of gw2 window and store it in mumble link.
/// 3. Dimensions of the gw2 window
///     This is something we get from mumble link and win32 api. We store this as client pos/size in mumble link
///     It is not just the width or height, but their ratio to the 1024x768 resolution
///
/// 1. By default, with dpi 96 (scale 1.0), at resolution 1024x768 these are the sizes of menu at different uisz settings
///     UISZ   -> WIDTH   HEIGHT
///     small  -> 288     27
///     normal -> 319     31
///     large  -> 355     34
///     larger -> 391     38
///     all units are in raw pixels.
///     
///     If we think of small uisz as the default. Then, we can express the rest of the sizes as ratio to small.
///     small = 1.0
///     normal = 1.1
///     large = 1.23
///     larger = 1.35
///     
///     So, just multiply small (288) with these ratios to get the actual pixels of each uisz.
/// 2. When dpi doubles, so do the sizes. 288 -> 576, 319 -> 638 etc.. So, when dpi scaling is enabled, we must multiply the above uisz ratio with dpi scale ratio to get the combined scaling ratio.
/// 3. The dimensions thing is a little complicated. So, i will just list the actual steps here.
///     1. take gw2's actual width in raw pixels. lets call this gw2_width.
///     2. take 1024 as reference minimum width. If dpi scaling is enabled, multiply 1024 * dpi scaling ratio. lets call this reference_width.
///     3. Now, get the smaller value out of the two. lets call this minimum_width.
///     4. finally, do (minimum_width / reference_width) to get "width scaling ratio".
///     5. repeat steps 1 - 4, but for height. use 768 as the reference width (with approapriate dpi scaling).
///     6. now just take the minimum of "width scaling ratio" and "height scaling ratio". lets call this "aspect ratio scaling".
///
/// Finally, just multiply the width 288 or height 27 with these three values.
/// eg: menu width = 288 * uisz_ratio * dpi_scaling_ratio * aspect_ratio_scaling;
/// do the same with 288 replaced by 27 for height.
#[derive(Debug, Default)]
pub struct MenuPanel {
    pub pos: egui::Pos2,
    pub ui_scaling_factor: f32,
    show_tracing_window: bool,
    show_theme_window: bool,
    // show_settings_window: bool,
    show_package_manager_window: bool,
    show_mumble_manager_window: bool,
    show_parameters_manager: bool,
    show_file_manager_window: bool,
}

impl MenuPanel {
    pub const WIDTH: f32 = 288.0;
    pub const HEIGHT: f32 = 27.0;
    pub fn tick(&mut self, etx: &egui::Context, link: Option<&jokolink::MumbleLink>) {
        let mut ui_scaling_factor = 1.0;
        if let Some(link) = link.as_ref() {
            let gw2_scale: f32 = if link.dpi_scaling == 1 || link.dpi_scaling == -1 {
                (if link.dpi == 0 { 96.0 } else { link.dpi as f32 }) / 96.0
            } else {
                1.0
            };

            ui_scaling_factor *= gw2_scale;
            let uisz_scale = convert_uisz_to_scale(link.uisz);
            ui_scaling_factor *= uisz_scale;

            let min_width = MINIMAL_WINDOW_WIDTH as f32 * gw2_scale;
            let min_height = MINIMAL_WINDOW_HEIGHT as f32 * gw2_scale;
            let gw2_width = link.client_size.x.max(MINIMAL_WINDOW_WIDTH) as f32;
            let gw2_height = link.client_size.y.max(MINIMAL_WINDOW_HEIGHT) as f32;
            let min_width_ratio = min_width.min(gw2_width) / min_width;
            let min_height_ratio = min_height.min(gw2_height) / min_height;

            let min_ratio = min_height_ratio.min(min_width_ratio);
            ui_scaling_factor *= min_ratio;

            let egui_scale = etx.pixels_per_point();
            ui_scaling_factor /= egui_scale;
        }

        self.pos.x = ui_scaling_factor * (Self::WIDTH + 8.0); // add 8 pixels padding just for some space
        self.ui_scaling_factor = ui_scaling_factor;
    }
}

fn convert_uisz_to_scale(uisize: jokolink::UISize) -> f32 {
    const SMALL: f32 = 288.0;
    const NORMAL: f32 = 319.0;
    const LARGE: f32 = 355.0;
    const LARGER: f32 = 391.0;
    const SMALL_SCALING_RATIO: f32 = 1.0;
    const NORMAL_SCALING_RATIO: f32 = NORMAL / SMALL;
    const LARGE_SCALING_RATIO: f32 = LARGE / SMALL;
    const LARGER_SCALING_RATIO: f32 = LARGER / SMALL;
    match uisize {
        jokolink::UISize::Small => SMALL_SCALING_RATIO,
        jokolink::UISize::Normal => NORMAL_SCALING_RATIO,
        jokolink::UISize::Large => LARGE_SCALING_RATIO,
        jokolink::UISize::Larger => LARGER_SCALING_RATIO,
    }
}
/*
Just some random measurements to verify in the future (or write tests for :))
with dpi enabled, there's some math involved it seems.
Linux ->
width 1920 pixels. height 2113 pixels. ratio 0.91. fov 1.01. scaling 2.0. dpi enabled
small  -> 540     53
normal -> 599     59
large  -> 667     65
larger -> 734     72


Windows ->
width 1920 pixels. height 2113 pixels. ratio 0.91. fov 1.01. scaling 2.0. dpi enabled.
small  -> 540     53
normal -> 599     59
large  -> 667     65
larger -> 734     72

width 1914 pixels. height 2072 pixels. ratio 0.92. fov 1.01. scaling 3.0. dpi enabled. dpi 288
small  -> 538     52
normal -> 598     58
large  -> 665     65
larger -> 731     72

width 3840. height 2160. ratio 1.78. scaling 3. dpi true. dpi 288 (windowed fullscreen)
small  -> 810     80
normal -> 900     89
large  -> 1000    99
larger -> 1100    109

width 1916 pixels. height 2113 pixels. ratio 0.91. fov 1.01. scaling 1.5. dpi enabled. dpi 144
small  -> 432     42
normal -> 480     47
large  -> 533     52
larger -> 586     57

width 1000 pixels. height 1000 pixels. ratio 1. fov 1.01. scaling 2.0. dpi enabled.
small  -> 281     26
normal -> 312     29
large  -> 347     33
larger -> 382     36

width 2000 pixels. height 1000 pixels. ratio 2. fov 1.01. scaling 2.0. dpi enabled.
small  -> 375     36
normal -> 416     40
large  -> 463     45
larger -> 509     49

width 2000 pixels. height 2000 pixels. ratio 1. fov 1.01. scaling 2.0. dpi enabled.
small  -> 562     55
normal -> 624     61
large  -> 694     68
larger -> 764     75


*/
