use std::borrow::BorrowMut;

use egui::DragValue;
use joko_component_models::{
    default_component_result, from_broadcast, to_data, Component, ComponentMessage, ComponentResult,
};
use joko_link_models::{MessageToMumbleLink, MumbleLink};
use joko_ui_models::{UIArea, UIPanel};

struct MumbleUIManagerChannels {
    subscription_mumble_link: tokio::sync::broadcast::Receiver<ComponentResult>,
    back_end_notifier: tokio::sync::mpsc::Sender<ComponentMessage>,
}

pub struct MumbleUIManager {
    egui_context: egui::Context,
    editable_mumble: bool,
    last_known_link: MumbleLink,
    channels: Option<MumbleUIManagerChannels>,
}

impl MumbleUIManager {
    pub fn new(egui_context: egui::Context) -> Self {
        Self {
            egui_context,
            editable_mumble: false,
            last_known_link: Default::default(),
            channels: None,
        }
    }
    fn live_mumble_ui(ui: &mut egui::Ui, mut link: MumbleLink) {
        egui::Grid::new("link grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                ui.label("ui tick");
                ui.add(DragValue::new(&mut link.ui_tick));
                ui.end_row();
                ui.label("player position");
                ui.horizontal(|ui| {
                    let player_pos = &mut link.player_pos.0;
                    ui.add(DragValue::new(&mut player_pos.x));
                    ui.add(DragValue::new(&mut player_pos.y));
                    ui.add(DragValue::new(&mut player_pos.z));
                });
                ui.end_row();
                ui.label("player direction");
                ui.horizontal(|ui| {
                    let f_avatar_front = &mut link.f_avatar_front.0;
                    ui.add(DragValue::new(&mut f_avatar_front.x));
                    ui.add(DragValue::new(&mut f_avatar_front.y));
                    ui.add(DragValue::new(&mut f_avatar_front.z));
                });
                ui.end_row();
                ui.label("camera position");
                ui.horizontal(|ui| {
                    let cam_pos = &mut link.cam_pos.0;
                    ui.add(DragValue::new(&mut cam_pos.x));
                    ui.add(DragValue::new(&mut cam_pos.y));
                    ui.add(DragValue::new(&mut cam_pos.z));
                });
                ui.end_row();
                ui.label("camera direction");
                ui.horizontal(|ui| {
                    let f_camera_front = &mut link.f_camera_front.0;
                    ui.add(DragValue::new(&mut f_camera_front.x));
                    ui.add(DragValue::new(&mut f_camera_front.y));
                    ui.add(DragValue::new(&mut f_camera_front.z));
                });
                ui.end_row();
                ui.label("ui state");
                if let Some(ui_state) = link.ui_state {
                    ui.label(ui_state.to_string());
                } else {
                    ui.label("None");
                }

                ui.end_row();
                ui.label("compass");
                ui.horizontal(|ui| {
                    ui.add(DragValue::new(&mut link.compass_height));
                    ui.add(DragValue::new(&mut link.compass_width));
                    ui.add(DragValue::new(&mut link.compass_rotation));
                });
                ui.end_row();

                ui.label("fov");
                ui.add(DragValue::new(&mut link.fov));
                ui.end_row();
                ui.label("w/h ratio");
                let ratio = link.client_size.0.as_vec2();
                let mut ratio = ratio.x / ratio.y;
                ui.add(DragValue::new(&mut ratio));
                ui.end_row();
                ui.label("character");
                ui.horizontal(|ui| {
                    ui.label(&link.name);
                    ui.label(format!("{:?}", link.race));
                });
                ui.end_row();

                ui.label("map id");
                ui.add(DragValue::new(&mut link.map_id));
                ui.end_row();
                ui.label("map type");
                ui.add(DragValue::new(&mut link.map_type));
                ui.end_row();
                ui.label("world position");
                ui.horizontal(|ui| {
                    ui.add(DragValue::new(&mut link.map_center_x));
                    ui.add(DragValue::new(&mut link.map_center_y));
                    ui.add(DragValue::new(&mut link.map_scale));
                });
                ui.end_row();

                ui.label("address");
                ui.label(format!("{}", link.server_address));
                ui.end_row();
                ui.label("instance");
                ui.add(DragValue::new(&mut link.instance));
                ui.end_row();
                ui.label("shard id");
                ui.add(DragValue::new(&mut link.shard_id));
                ui.end_row();
                ui.label("mount");
                ui.label(format!("{:?}", link.mount));
                ui.end_row();
                ui.label("client pos");
                ui.horizontal(|ui| {
                    let client_pos = &mut link.client_pos.0;
                    ui.add(DragValue::new(&mut client_pos.x));
                    ui.add(DragValue::new(&mut client_pos.y));
                });
                ui.end_row();
                ui.label("client size");
                ui.horizontal(|ui| {
                    let client_size = &mut link.client_size.0;
                    ui.add(DragValue::new(&mut client_size.x));
                    ui.add(DragValue::new(&mut client_size.y));
                });
                ui.end_row();
                ui.label("dpi scaling");
                ui.add(DragValue::new(&mut link.dpi_scaling));
                ui.end_row();
                ui.label("dpi");
                ui.add(DragValue::new(&mut link.dpi));
                ui.end_row();
            });
    }

    fn editable_mumble_ui(ui: &mut egui::Ui, dummy_link: &mut MumbleLink) {
        egui::Grid::new("link grid")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                ui.label("ui tick");
                ui.add(DragValue::new(&mut dummy_link.ui_tick));
                ui.end_row();
                ui.label("player position");
                ui.horizontal(|ui| {
                    let player_pos = &mut dummy_link.player_pos.0;
                    ui.add(DragValue::new(&mut player_pos.x));
                    ui.add(DragValue::new(&mut player_pos.y));
                    ui.add(DragValue::new(&mut player_pos.z));
                });
                ui.end_row();
                ui.label("player direction");
                ui.horizontal(|ui| {
                    let f_avatar_front = &mut dummy_link.f_avatar_front.0;
                    ui.add(DragValue::new(&mut f_avatar_front.x));
                    ui.add(DragValue::new(&mut f_avatar_front.y));
                    ui.add(DragValue::new(&mut f_avatar_front.z));
                });
                ui.end_row();
                ui.label("camera position");
                ui.horizontal(|ui| {
                    let cam_pos = &mut dummy_link.cam_pos.0;
                    ui.add(DragValue::new(&mut cam_pos.x));
                    ui.add(DragValue::new(&mut cam_pos.y));
                    ui.add(DragValue::new(&mut cam_pos.z));
                });
                ui.end_row();
                ui.label("camera direction");
                ui.horizontal(|ui| {
                    let f_camera_front = &mut dummy_link.f_camera_front.0;
                    ui.add(DragValue::new(&mut f_camera_front.x));
                    ui.add(DragValue::new(&mut f_camera_front.y));
                    ui.add(DragValue::new(&mut f_camera_front.z));
                });
                ui.end_row();

                ui.label("ui state");
                if let Some(ui_state) = dummy_link.ui_state {
                    ui.label(ui_state.to_string());
                } else {
                    ui.label("None");
                }

                ui.end_row();
                ui.label("compass");
                ui.horizontal(|ui| {
                    ui.add(DragValue::new(&mut dummy_link.compass_height));
                    ui.add(DragValue::new(&mut dummy_link.compass_width));
                    ui.add(DragValue::new(&mut dummy_link.compass_rotation));
                });
                ui.end_row();

                ui.label("fov");
                ui.add(DragValue::new(&mut dummy_link.fov));
                ui.end_row();
                ui.label("w/h ratio");
                let ratio = dummy_link.client_size.0.as_vec2();
                let mut ratio = ratio.x / ratio.y;
                ui.add(DragValue::new(&mut ratio));
                ui.end_row();
                ui.label("character");
                ui.label(&dummy_link.name);
                ui.end_row();
                ui.label("map id");
                ui.add(DragValue::new(&mut dummy_link.map_id));
                ui.end_row();
                ui.label("map type");
                ui.add(DragValue::new(&mut dummy_link.map_type));
                ui.end_row();
                ui.label("address");
                ui.label(format!("{}", dummy_link.server_address));
                ui.end_row();
                ui.label("instance");
                ui.add(DragValue::new(&mut dummy_link.instance));
                ui.end_row();
                ui.label("shard id");
                ui.add(DragValue::new(&mut dummy_link.shard_id));
                ui.end_row();
                ui.label("mount");
                ui.label(format!("{:?}", dummy_link.mount));
                ui.end_row();
                ui.label("client pos");
                ui.horizontal(|ui| {
                    let client_pos = &mut dummy_link.client_pos.0;
                    ui.add(DragValue::new(&mut client_pos.x));
                    ui.add(DragValue::new(&mut client_pos.y));
                });
                ui.end_row();
                ui.label("client size");
                ui.horizontal(|ui| {
                    let client_size = &mut dummy_link.client_size.0;
                    ui.add(DragValue::new(&mut client_size.x));
                    ui.add(DragValue::new(&mut client_size.y));
                });
                ui.end_row();
                ui.label("dpi scaling");
                ui.add(DragValue::new(&mut dummy_link.dpi_scaling));
                ui.end_row();
                ui.label("dpi");
                ui.add(DragValue::new(&mut dummy_link.dpi));
                ui.end_row();

                // ui.label("position");
                // ui.horizontal(|ui| {
                //     ui.add(DragValue::new(&mut link.window_pos.x));
                //     ui.add(DragValue::new(&mut link.window_pos.y));
                // });
                // ui.end_row();
                // ui.label("size");
                // ui.horizontal(|ui| {
                //     ui.add(DragValue::new(&mut link.window_size.x));
                //     ui.add(DragValue::new(&mut link.window_size.y));
                // });
                // ui.end_row();
                // ui.label("position_nb");
                // ui.horizontal(|ui| {
                //     ui.add(DragValue::new(&mut link.window_pos_without_borders.x));
                //     ui.add(DragValue::new(&mut link.window_pos_without_borders.y));
                // });
                // ui.end_row();
                // ui.label("size_nb");
                // ui.horizontal(|ui| {
                //     ui.add(DragValue::new(&mut link.window_size_without_borders.x));
                //     ui.add(DragValue::new(&mut link.window_size_without_borders.y));
                // });
                // ui.end_row();
            });
    }
}

impl Component for MumbleUIManager {
    fn bind(&mut self, mut channels: joko_component_models::ComponentChannels) {
        let channels = MumbleUIManagerChannels {
            subscription_mumble_link: channels.requirements.remove(&0).unwrap(),
            back_end_notifier: channels.notify.remove(&1).unwrap(),
        };
        self.channels = Some(channels);
    }
    fn flush_all_messages(&mut self) {
        assert!(
            self.channels.is_some(),
            "channels must be initialized before interacting with component."
        );
    }
    fn accept_notifications(&self) -> bool {
        false
    }
    fn init(&mut self) {}
    fn requirements(&self) -> Vec<&str> {
        vec!["ui:mumble_link"]
    }
    fn notify(&self) -> Vec<&str> {
        vec!["back:mumble_link"]
    }
    fn peers(&self) -> Vec<&str> {
        vec![]
    }
    fn tick(&mut self, _latest_time: f64) -> joko_component_models::ComponentResult {
        assert!(
            self.channels.is_some(),
            "channels must be initialized before interacting with component."
        );
        let channels = self.channels.as_mut().unwrap();

        if let Ok(link) = channels.subscription_mumble_link.try_recv() {
            let link: Option<MumbleLink> = from_broadcast(&link);
            if self.editable_mumble {
            } else if let Some(link) = link {
                self.last_known_link = link;
            }
        }
        default_component_result()
    }
}

impl UIPanel for MumbleUIManager {
    fn areas(&self) -> Vec<UIArea> {
        vec![UIArea {
            is_open: false,
            name: "Mumble Manager".to_string(),
            id: "mumble_ui".to_string(),
        }]
    }
    fn init(&mut self) {}
    fn gui(&mut self, is_open: &mut bool, _area_id: &str, _latest_time: f64) {
        assert!(
            self.channels.is_some(),
            "channels must be initialized before interacting with component."
        );
        let channels = self.channels.as_mut().unwrap();
        let back_end_notifier = channels.back_end_notifier.borrow_mut();
        let egui_context = &self.egui_context;

        egui::Window::new("Mumble Manager")
            .open(is_open)
            .show(egui_context, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(!self.editable_mumble, "live").clicked() {
                        self.editable_mumble = false;
                        let _ = back_end_notifier
                            .blocking_send(to_data(MessageToMumbleLink::Autonomous));
                    }
                    if ui
                        .selectable_label(self.editable_mumble, "editable")
                        .clicked()
                    {
                        self.editable_mumble = true;
                        let _ = back_end_notifier
                            .blocking_send(to_data(MessageToMumbleLink::BindedOnUI));
                    }
                });
                if self.editable_mumble {
                    ui.label(
                        egui::RichText::new(
                            "Mumble is not live, values need to be manually updated.",
                        )
                        .color(egui::Color32::RED),
                    );
                    //TODO: how to detect there was a change in value, to only propagate changed values ?
                    Self::editable_mumble_ui(ui, &mut self.last_known_link);
                } else {
                    let link: MumbleLink = self.last_known_link.clone();
                    Self::live_mumble_ui(ui, link);
                }
            });
        if self.editable_mumble {
            let _ = back_end_notifier.blocking_send(to_data(MessageToMumbleLink::Value(
                self.last_known_link.clone(),
            )));
        }
    }
}
