use egui_window_glfw_passthrough::GlfwBackend;

use jmf::message::UIToBackMessage;
use serde::{Deserialize, Serialize};

pub const UI_PARAMETERS_FILE_NAME: &str = "ui.toml";

#[derive(Serialize, Deserialize)]
pub struct JokolayUIParameters {
    pub visible_borders: bool,
    pub animate: bool,
    pub editable_path: String,
    //TODO: folder path for custom work directory
    //save configuration into a file + make backups of configuration
}

pub struct JokolayUIConfiguration {
    pub fps_last_reset: f64,
    pub frame_count: u32,
    pub total_frame_count: u32,
    pub average_fps: u32,
    pub display_parameters: JokolayUIParameters,
}

impl JokolayUIConfiguration {
    pub fn new(current_time: f64, editable_path: String) -> Self {
        Self {
            fps_last_reset: current_time,
            frame_count: 0,
            total_frame_count: 0,
            average_fps: 0,
            display_parameters: JokolayUIParameters {
                visible_borders: false,
                animate: true,
                editable_path,
            },
        }
    }

    pub fn tick(&mut self, current_time: f64) {
        self.total_frame_count += 1;
        self.frame_count += 1;
        if current_time - self.fps_last_reset > 1.0 {
            self.average_fps = self.frame_count;
            self.frame_count = 0;
            self.fps_last_reset = current_time;
        }
    }

    pub fn gui(
        &mut self,
        u2b_sender: &std::sync::mpsc::Sender<UIToBackMessage>,
        etx: &egui::Context,
        wb: &mut GlfwBackend,
        open: &mut bool,
        root_path: &std::path::PathBuf,
    ) {
        let mut need_to_save = false;
        egui::Window::new("Configuration")
            .open(open)
            .show(etx, |ui| {
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
                            wb.window_position[0], wb.window_position[1]
                        ));
                        ui.end_row();
                        ui.label("Overlay size");
                        ui.label(&format!(
                            "width: {}, height: {}",
                            wb.framebuffer_size_physical[0], wb.framebuffer_size_physical[1]
                        ));
                        ui.end_row();

                        ui.label("Decorations (borders)")
                            .on_hover_text("Should the jokolay overlay window boreders be displayed");
                        let is_decorated = wb.window.is_decorated();
                        ui.horizontal(|ui|{
                            let result = is_decorated;
                            if ui.selectable_label(result, "Visible").clicked() {
                                wb.window.set_decorated(true);
                                self.display_parameters.visible_borders = true;
                                need_to_save = true;
                            }
                            if ui.selectable_label(!result, "Hidden").clicked() {
                                wb.window.set_decorated(false);
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
                        ui.label(root_path.to_str().unwrap());
                        ui.end_row();

                        ui.label("Editable package directory")
                            .on_hover_text_at_pointer("This is where you can manually edit a package and have it regularly imported for validation.");
                        ui.text_edit_singleline(&mut self.display_parameters.editable_path);
                    });
            });
        if need_to_save {
            match toml::to_string(&self.display_parameters) {
                Ok(serialized_string) => {
                    let _ =
                        u2b_sender.send(UIToBackMessage::SaveUIConfiguration(serialized_string));
                }
                Err(e) => {
                    tracing::error!(?e, "failed to serialize UI configuration");
                }
            }
        }
    }
}
