//! Jokolink is a crate to deal with Mumble Link data exposed by games/apps on windows via shared memory

//! Joko link is designed to primarily get the MumbleLink or the window size
//! of the GW2 window for Jokolay (an crossplatform overlay for Guild Wars 2).
//! on windows, you can use it to create/open shared memory.
//! and on linux, you can run jokolink binary in wine, which will create/open shared memory and copy-paste it into /dev/shm.
//! then, you can easily read the /dev/shm file from a any number of linux native applications.
//! along with mumblelink data, it also copies the x11 window id of gw2. you can use this to get the size of gw2 window.
//!

mod mumble;
use egui::DragValue;
use enumflags2::BitFlags;
use glam::IVec2;
//use jokoapi::end_point::{mounts::Mount, races::Race};
use miette::{IntoDiagnostic, Result, WrapErr};
pub use mumble::*;
use serde_json::from_str;
use tracing::error;

/// The default mumble link name. can only be changed by passing the `-mumble` options to gw2 for multiboxing
pub const DEFAULT_MUMBLELINK_NAME: &str = "MumbleLink";
#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "windows")]
pub mod win;

#[cfg(target_os = "linux")]
use linux::MumbleLinuxImpl as MumblePlatformImpl;
#[cfg(target_os = "windows")]
use win::MumbleWinImpl as MumblePlatformImpl;
// Useful link size is only [ctypes::USEFUL_C_MUMBLE_LINK_SIZE] . And we add 100 more bytes so that jokolink can put some extra stuff in there
// pub(crate) const JOKOLINK_MUMBLE_BUFFER_SIZE: usize = ctypes::USEFUL_C_MUMBLE_LINK_SIZE + 100;
/// This primarily manages the mumble backend.
/// the purpose of `MumbleBackend` is to get mumble link data and window dimensions when asked.
/// Manager also caches the previous mumble link details like window dimensions or mapid etc..
/// and every frame gets the latest mumble link data, and compares with the previous frame.
/// if any of the changed this frame, it will set the relevant changed flags so that plugins
/// or other parts of program which care can run the relevant code.
pub struct MumbleManager {
    /// This abstracts over the windows and linux impl of mumble link functionality.
    /// we use this to get the latest mumble link and latest window dimensions of the current mumble link
    backend: MumblePlatformImpl,
    /// latest mumble link
    link: MumbleLink,

}
impl MumbleManager {
    pub fn new(name: &str, _jokolay_window_id: Option<u32>) -> Result<Self> {
        let backend = MumblePlatformImpl::new(name)?;
        Ok(Self {
            backend,
            link: Default::default(),
        })
    }
    pub fn is_alive(&self) -> bool {
        self.backend.is_alive()
    }

    pub fn tick(&mut self) -> Result<Option<&MumbleLink>> {
        if let Err(e) = self.backend.tick() {
            error!(?e, "mumble backend tick error");
            return Ok(None);
        }

        if !self.backend.is_alive() {
            self.link.client_size.x = self.link.client_size.x.max(1024);
            self.link.client_size.y = self.link.client_size.y.max(768);
            self.link.changes = BitFlags::all();
            return Ok(Some(&self.link));
        }
        // backend is alive and tick is successful. time to get link
        let cml: ctypes::CMumbleLink = self.backend.get_cmumble_link();
        let mut new_link = if cml.ui_tick == 0 && self.link.ui_tick != 0 {
            Default::default()
        } else {
            self.link.clone()
        };

        if cml.ui_tick == 0 || cml.context.client_pos_size == [0; 4] {
            return Ok(None);
        }
        let mut changes: BitFlags<MumbleChanges> = Default::default();
        // safety. as the link is valid, we can use as_ref
        let json_string = widestring::U16CStr::from_slice_truncate(&cml.identity)
            .into_diagnostic()
            .wrap_err("failed to get widestring out of cml identity")?
            .to_string()
            .into_diagnostic()
            .wrap_err("failed to convert widestring to cstring")?;

        let identity: ctypes::CIdentity = from_str(&json_string)
            .into_diagnostic()
            .wrap_err("failed to deserialize identity from json string")?;
        let uisz = identity
            .get_uisz()
            .ok_or(miette::miette!("uisz is invalid"))?;
        let server_address = if cml.context.server_address[0] == 2 {
            let addr = cml.context.server_address;
            std::net::Ipv4Addr::new(addr[4], addr[5], addr[6], addr[7]).into()
        } else {
            std::net::Ipv4Addr::UNSPECIFIED.into()
        };
        if new_link.ui_tick != cml.ui_tick {
            changes.insert(MumbleChanges::UiTick);
        }
        if new_link.name != identity.name {
            changes.insert(MumbleChanges::Character);
        }
        if new_link.map_id != cml.context.map_id {
            changes.insert(MumbleChanges::Map);
        }
        let client_pos = IVec2::new(
            cml.context.client_pos_size[0],
            cml.context.client_pos_size[1],
        );
        let client_size = IVec2::new(
            cml.context.client_pos_size[2],
            cml.context.client_pos_size[3],
        );

        if new_link.client_pos != client_pos {
            changes.insert(MumbleChanges::WindowPosition);
        }
        if new_link.client_size != client_size {
            changes.insert(MumbleChanges::WindowSize);
        }
        let cam_pos = cml.f_camera_position.into();
        if new_link.cam_pos != cam_pos {
            changes.insert(MumbleChanges::Camera);
        }

        let player_pos = cml.f_avatar_position.into();
        if new_link.player_pos != player_pos {
            changes.insert(MumbleChanges::Position);
        }
        //let player_race = Self::get_race(identity.race);

        new_link = MumbleLink {
            ui_tick: cml.ui_tick,
            player_pos,
            f_avatar_front: cml.f_avatar_front.into(),
            cam_pos,
            f_camera_front: cml.f_camera_front.into(),
            name: identity.name,
            map_id: cml.context.map_id,
            fov: identity.fov,
            uisz,
            // window_pos,
            // window_size,
            changes,
            // window_pos_without_borders,
            // window_size_without_borders,
            dpi_scaling: cml.context.dpi_scaling,
            dpi: cml.context.dpi,
            client_pos,
            client_size,
            map_type: cml.context.map_type,
            server_address,
            shard_id: cml.context.shard_id,
            instance: cml.context.instance,
            build_id: cml.context.build_id,
            ui_state: cml.context.get_ui_state(),
            compass_width: cml.context.compass_width,
            compass_height: cml.context.compass_height,
            compass_rotation: cml.context.compass_rotation,
            player_x: cml.context.player_x,
            player_y: cml.context.player_y,
            map_center_x: cml.context.map_center_x,
            map_center_y: cml.context.map_center_y,
            map_scale: cml.context.map_scale,
            process_id: cml.context.process_id,
            mount: cml.context.mount_index,
            race: identity.race,
        };
        self.link = new_link;

        Ok(if self.link.ui_tick == 0 {
            None
        } else {
            Some(&self.link)
        })
    }
}

pub fn mumble_gui(etx: &egui::Context, open: &mut bool, editable_mumble: &mut bool, link: &mut MumbleLink) {
    egui::Window::new("Mumble Manager")
        .open(open)
        .show(etx, |ui| {
            if *editable_mumble {
                if ui.button("back to live").clicked() {
                    *editable_mumble = false;
                }
                ui.label(
                    egui::RichText::new("Mumble is not initialized, display dummy link instead.")
                    .color(egui::Color32::RED)
                );
                editable_mumble_ui(ui, link);
            } else {
                if ui.button("go to edit mode").clicked() {
                    *editable_mumble = true;
                }
                let link: MumbleLink = link.clone();
                mumble_ui(ui, link);
            }
        });
}

fn mumble_ui(ui: &mut egui::Ui, mut link: MumbleLink) {
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
