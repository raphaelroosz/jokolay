use cap_std::fs_utf8::Dir;
use egui_window_glfw_passthrough::GlfwBackend;
use std::{
    io::Write,
    sync::{Arc, RwLock},
};

use joko_component_models::{
    default_component_result, from_data, to_data, Component, ComponentMessage, ComponentResult,
};
use joko_ui_models::{UIArea, UIPanel};
use miette::IntoDiagnostic;
use serde::{Deserialize, Serialize};
use tracing::error;

pub const UI_PARAMETERS_FILE_NAME: &str = "ui.toml";

#[derive(Clone, Serialize, Deserialize)]
pub enum MessageToApplicationBack {
    SaveUIConfiguration(String),
}

#[derive(Serialize, Deserialize)]
pub struct JokolayUIParameters {
    pub visible_borders: bool,
    pub animate: bool, //FIXME: not linked to animation anymore
    pub editable_path: String,
    pub root_path: String,
    //TODO: save configuration into a file + make backups of configuration
}

struct JokolayUIConfigurationChannels {
    back_end_notifier: tokio::sync::mpsc::Sender<ComponentMessage>,
}
struct JokolayConfigurationChannels {
    notification_receiver: tokio::sync::mpsc::Receiver<ComponentMessage>,
}

pub struct JokolayUIConfiguration {
    pub fps_last_reset: f64,
    pub frame_count: u32,
    pub total_frame_count: u32,
    pub average_fps: u32,
    pub display_parameters: JokolayUIParameters,
    glfw_backend: Arc<RwLock<GlfwBackend>>,
    egui_context: egui::Context,
    channels: Option<JokolayUIConfigurationChannels>,
}

pub struct JokolayConfiguration {
    root_dir: Arc<Dir>,
    channels: Option<JokolayConfigurationChannels>,
}

/// Necessary lies for GlfwBackend, which despite not moved (Arc + Mutex) shall prevent compilation
unsafe impl Send for JokolayUIConfiguration {}
unsafe impl Sync for JokolayUIConfiguration {}

impl JokolayUIConfiguration {
    pub fn new(
        glfw_backend: Arc<RwLock<GlfwBackend>>,
        egui_context: egui::Context,
        editable_path: String,
        root_path: String,
    ) -> Self {
        let fps_last_reset: f64 = { glfw_backend.read().unwrap().glfw.get_time() as _ };
        Self {
            fps_last_reset,
            frame_count: 0,
            total_frame_count: 0,
            average_fps: 0,
            display_parameters: JokolayUIParameters {
                visible_borders: false,
                animate: true,
                editable_path,
                root_path,
            },
            glfw_backend,
            egui_context,
            channels: None,
        }
    }
}

impl Component for JokolayUIConfiguration {
    fn accept_notifications(&self) -> bool {
        true
    }
    fn bind(&mut self, mut channels: joko_component_models::ComponentChannels) {
        let back_end_notifier = channels.notify.remove(&0).unwrap();
        let channels = JokolayUIConfigurationChannels { back_end_notifier };
        self.channels = Some(channels)
    }
    fn flush_all_messages(&mut self) {}

    fn init(&mut self) {}

    fn tick(&mut self, current_time: f64) -> ComponentResult {
        self.total_frame_count += 1;
        self.frame_count += 1;
        if current_time - self.fps_last_reset > 1.0 {
            self.average_fps = self.frame_count;
            self.frame_count = 0;
            self.fps_last_reset = current_time;
        }
        default_component_result()
    }
    fn notify(&self) -> Vec<&str> {
        vec!["back:configuration"]
    }
}

impl JokolayConfiguration {
    pub fn new(root_dir: Arc<Dir>) -> Self {
        Self {
            root_dir,
            channels: None,
        }
    }
    fn handle_message(&mut self, msg: MessageToApplicationBack) {
        let root_dir = &self.root_dir;
        match msg {
            MessageToApplicationBack::SaveUIConfiguration(serialized_string) => {
                match root_dir.create(UI_PARAMETERS_FILE_NAME) {
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
            #[allow(unreachable_patterns)]
            _ => {
                unimplemented!("Handling BackToUIMessage has not been implemented yet");
            }
        }
    }
}

impl Component for JokolayConfiguration {
    fn accept_notifications(&self) -> bool {
        true
    }
    fn init(&mut self) {}
    fn flush_all_messages(&mut self) {
        assert!(
            self.channels.is_some(),
            "channels must be initialized before interacting with component."
        );
        let channels = self.channels.as_mut().unwrap();
        let mut messages = Vec::new();
        while let Ok(msg) = channels.notification_receiver.try_recv() {
            messages.push(from_data(&msg));
        }
        for msg in messages {
            self.handle_message(msg);
        }
    }
    fn bind(&mut self, channels: joko_component_models::ComponentChannels) {
        let channels = JokolayConfigurationChannels {
            notification_receiver: channels.input_notification.unwrap(),
        };
        self.channels = Some(channels);
    }
    fn tick(&mut self, _latest_time: f64) -> joko_component_models::ComponentResult {
        default_component_result()
    }
}

impl UIPanel for JokolayUIConfiguration {
    fn areas(&self) -> Vec<UIArea> {
        vec![UIArea {
            is_open: false,
            name: "Configuration".to_string(),
            id: "configuration_ui".to_string(),
        }]
    }
    fn init(&mut self) {}

    fn gui(&mut self, is_open: &mut bool, _area_id: &str, _latest_time: f64) {
        let channels = self.channels.as_mut().unwrap();
        let u2b_sender = &channels.back_end_notifier;
        let glfw_backend = Arc::clone(&self.glfw_backend);
        let mut glfw_backend = glfw_backend.as_ref().write().unwrap();
        let mut need_to_save = false;
        egui::Window::new("Configuration")
            .open(is_open)
            .show(&self.egui_context, |ui| {
                egui::Grid::new("frame details")
                    .num_columns(2)
                    .show(ui, |ui| {
                        ui.label("FPS");
                        ui.label(&format!("{}", self.average_fps));
                        ui.end_row();
                        ui.label("Frame count");
                        ui.label(&format!("{}", self.total_frame_count));
                        ui.end_row();
                        ui.label("Overlay position");
                        ui.label(&format!(
                            "x: {}; y: {}",
                            glfw_backend.window_position[0], glfw_backend.window_position[1]
                        ));
                        ui.end_row();
                        ui.label("Overlay size");
                        ui.label(&format!(
                            "width: {}, height: {}",
                            glfw_backend.framebuffer_size_physical[0], glfw_backend.framebuffer_size_physical[1]
                        ));
                        ui.end_row();

                        ui.label("Decorations (borders)")
                            .on_hover_text("Should the jokolay overlay window boreders be displayed");
                        let is_decorated = glfw_backend.window.is_decorated();
                        ui.horizontal(|ui|{
                            let result = is_decorated;
                            if ui.selectable_label(result, "Visible").clicked() {
                                glfw_backend.window.set_decorated(true);
                                self.display_parameters.visible_borders = true;
                                need_to_save = true;
                            }
                            if ui.selectable_label(!result, "Hidden").clicked() {
                                glfw_backend.window.set_decorated(false);
                                self.display_parameters.visible_borders = false;
                                need_to_save = true;
                            }
                        });
                        ui.end_row();

                        ui.label("Animation")
                            .on_hover_text("As an example, this toggle the animation of trails");
                        ui.horizontal(|ui|{
                            if ui.selectable_label(self.display_parameters.animate, "Enable").clicked() {
                                self.display_parameters.animate = true;
                                need_to_save = true;
                            }
                            if ui.selectable_label(!self.display_parameters.animate, "Disable").clicked() {
                                self.display_parameters.animate = false;
                                need_to_save = true;
                            }
                        });
                        ui.end_row();
                        ui.label("All files and preferences are saved into:");
                        ui.label(&self.display_parameters.root_path);
                        ui.end_row();

                        ui.label("Editable package directory")
                            .on_hover_text_at_pointer("This is where you can manually edit a package and have it regularly imported for validation.");
                        ui.text_edit_singleline(&mut self.display_parameters.editable_path);
                    });
            });
        if need_to_save {
            match toml::to_string(&self.display_parameters) {
                Ok(serialized_string) => {
                    let _ = u2b_sender.blocking_send(to_data(
                        MessageToApplicationBack::SaveUIConfiguration(serialized_string),
                    ));
                }
                Err(e) => {
                    tracing::error!(?e, "failed to serialize UI configuration");
                }
            }
        }
    }
}
