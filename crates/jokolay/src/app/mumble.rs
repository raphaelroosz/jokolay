
use egui;
use egui::DragValue;
use jmf::message::UIToBackMessage;
use jokolink::MumbleLink;


pub fn mumble_gui(
    u2b_sender: &std::sync::mpsc::Sender<UIToBackMessage>,
    etx: &egui::Context, 
    open: &mut bool,
    editable_mumble: &mut bool, 
    link: &mut MumbleLink
) {
    egui::Window::new("Mumble Manager")
        .open(open)
        .show(etx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(!*editable_mumble, "live").clicked() {
                    *editable_mumble = false;
                    u2b_sender.send(UIToBackMessage::MumbleLinkAutonomous);
                }
                if ui.selectable_label(*editable_mumble, "editable").clicked() {
                    *editable_mumble = true;
                    u2b_sender.send(UIToBackMessage::MumbleLinkBindedOnUI);
                }
            });
            if *editable_mumble {
                ui.label(
                    egui::RichText::new("Mumble is not live, values need to be manually updated.")
                    .color(egui::Color32::RED)
                );
                editable_mumble_ui(ui, link);
            } else {
                let link: MumbleLink = link.clone();
                live_mumble_ui(ui, link);
            }
        });
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
                ui.add(DragValue::new(&mut link.player_pos.x));
                ui.add(DragValue::new(&mut link.player_pos.y));
                ui.add(DragValue::new(&mut link.player_pos.z));
            });
            ui.end_row();
            ui.label("player direction");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut link.f_avatar_front.x));
                ui.add(DragValue::new(&mut link.f_avatar_front.y));
                ui.add(DragValue::new(&mut link.f_avatar_front.z));
            });
            ui.end_row();
            ui.label("camera position");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut link.cam_pos.x));
                ui.add(DragValue::new(&mut link.cam_pos.y));
                ui.add(DragValue::new(&mut link.cam_pos.z));
            });
            ui.end_row();
            ui.label("camera direction");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut link.f_camera_front.x));
                ui.add(DragValue::new(&mut link.f_camera_front.y));
                ui.add(DragValue::new(&mut link.f_camera_front.z));
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
            ui.horizontal(|ui|{
                ui.add(DragValue::new(&mut link.compass_height));
                ui.add(DragValue::new(&mut link.compass_width));
                ui.add(DragValue::new(&mut link.compass_rotation));
            });
            ui.end_row();

            ui.label("fov");
            ui.add(DragValue::new(&mut link.fov));
            ui.end_row();
            ui.label("w/h ratio");
            let ratio = link.client_size.as_vec2();
            let mut ratio = ratio.x / ratio.y;
            ui.add(DragValue::new(&mut ratio));
            ui.end_row();
            ui.label("character");
            ui.horizontal(|ui|{
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
            ui.horizontal(|ui|{
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
                ui.add(DragValue::new(&mut link.client_pos.x));
                ui.add(DragValue::new(&mut link.client_pos.y));
            });
            ui.end_row();
            ui.label("client size");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut link.client_size.x));
                ui.add(DragValue::new(&mut link.client_size.y));
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
                ui.add(DragValue::new(&mut dummy_link.player_pos.x));
                ui.add(DragValue::new(&mut dummy_link.player_pos.y));
                ui.add(DragValue::new(&mut dummy_link.player_pos.z));
            });
            ui.end_row();
            ui.label("player direction");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut dummy_link.f_avatar_front.x));
                ui.add(DragValue::new(&mut dummy_link.f_avatar_front.y));
                ui.add(DragValue::new(&mut dummy_link.f_avatar_front.z));
            });
            ui.end_row();
            ui.label("camera position");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut dummy_link.cam_pos.x));
                ui.add(DragValue::new(&mut dummy_link.cam_pos.y));
                ui.add(DragValue::new(&mut dummy_link.cam_pos.z));
            });
            ui.end_row();
            ui.label("camera direction");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut dummy_link.f_camera_front.x));
                ui.add(DragValue::new(&mut dummy_link.f_camera_front.y));
                ui.add(DragValue::new(&mut dummy_link.f_camera_front.z));
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
            ui.horizontal(|ui|{
                ui.add(DragValue::new(&mut dummy_link.compass_height));
                ui.add(DragValue::new(&mut dummy_link.compass_width));
                ui.add(DragValue::new(&mut dummy_link.compass_rotation));
            });
            ui.end_row();

            ui.label("fov");
            ui.add(DragValue::new(&mut dummy_link.fov));
            ui.end_row();
            ui.label("w/h ratio");
            let ratio = dummy_link.client_size.as_vec2();
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
                ui.add(DragValue::new(&mut dummy_link.client_pos.x));
                ui.add(DragValue::new(&mut dummy_link.client_pos.y));
            });
            ui.end_row();
            ui.label("client size");
            ui.horizontal(|ui| {
                ui.add(DragValue::new(&mut dummy_link.client_size.x));
                ui.add(DragValue::new(&mut dummy_link.client_size.y));
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
