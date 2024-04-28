//! Jokolink is a crate to deal with Mumble Link data exposed by games/apps on windows via shared memory

//! Joko link is designed to primarily get the MumbleLink or the window size
//! of the GW2 window for Jokolay (an crossplatform overlay for Guild Wars 2).
//! on windows, you can use it to create/open shared memory.
//! and on linux, you can run jokolink binary in wine, which will create/open shared memory and copy-paste it into /dev/shm.
//! then, you can easily read the /dev/shm file from a any number of linux native applications.
//! along with mumblelink data, it also copies the x11 window id of gw2. you can use this to get the size of gw2 window.
//!

use std::vec;

use enumflags2::BitFlags;
use joko_component_models::{
    from_data, to_data, ComponentDataExchange, JokolayComponent, JokolayComponentDeps,
    PeerComponentChannel,
};
use joko_core::serde_glam::{IVec2, UVec2, Vec3};
use joko_link_models::{
    ctypes, MessageToMumbleLinkBack, MumbleChanges, MumbleLink, MumbleLinkResult,
};
//use jokoapi::end_point::{mounts::Mount, races::Race};
use miette::{IntoDiagnostic, Result, WrapErr};
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

struct MumbleChannels {
    notification_receiver: tokio::sync::mpsc::Receiver<ComponentDataExchange>,
}
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
    is_ui: bool,
    /// latest mumble link
    link: MumbleLink,

    channels: Option<MumbleChannels>,
    state: MumbleLinkResult,
}

impl MumbleManager {
    pub fn new(name: &str, is_ui: bool) -> Result<Self> {
        let backend = MumblePlatformImpl::new(name)?;
        Ok(Self {
            backend,
            link: Default::default(),
            channels: None,
            is_ui,
            state: MumbleLinkResult {
                read_ui_link: true,
                link: None,
                ui_link: None,
            },
        })
    }
    pub fn is_alive(&self) -> bool {
        self.backend.is_alive()
    }
    fn handle_message(&mut self, msg: MessageToMumbleLinkBack) {
        //let (b2u_sender, _) = package_manager.channels();
        match msg {
            MessageToMumbleLinkBack::Autonomous => {
                tracing::trace!("Handling of UIToBackMessage::MumbleLinkAutonomous");
                self.state.read_ui_link = false;
            }
            MessageToMumbleLinkBack::BindedOnUI => {
                tracing::trace!("Handling of UIToBackMessage::MumbleLinkBindedOnUI");
                self.state.read_ui_link = true;
            }
            MessageToMumbleLinkBack::Value(link) => {
                tracing::trace!("Handling of UIToBackMessage::MumbleLink");
                self.state.ui_link = link;
            }
            #[allow(unreachable_patterns)]
            _ => {
                unimplemented!("Handling MessageToPackageBack has not been implemented yet");
            }
        }
    }
    fn _tick(&mut self) -> Result<Option<&MumbleLink>> {
        if let Err(e) = self.backend.tick() {
            error!(?e, "mumble backend tick error");
            return Ok(None);
        }

        if !self.backend.is_alive() {
            self.link.client_size.0.x = 0;
            self.link.client_size.0.y = 0;
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

        if cml.ui_tick == 0 || cml.context.client_pos == [0; 2] {
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
        let client_pos = IVec2(glam::IVec2::new(
            cml.context.client_pos[0],
            cml.context.client_pos[1],
        ));
        let client_size = UVec2(glam::UVec2::new(
            cml.context.client_size[0],
            cml.context.client_size[1],
        ));

        if new_link.client_pos != client_pos {
            changes.insert(MumbleChanges::WindowPosition);
        }
        if new_link.client_size != client_size {
            changes.insert(MumbleChanges::WindowSize);
        }
        let cam_pos: glam::Vec3 = cml.f_camera_position.into();
        if new_link.cam_pos.0 != cam_pos {
            changes.insert(MumbleChanges::Camera);
        }

        let player_pos: glam::Vec3 = cml.f_avatar_position.into();
        if new_link.player_pos.0 != player_pos {
            changes.insert(MumbleChanges::Position);
        }
        //let player_race = Self::get_race(identity.race);

        new_link = MumbleLink {
            ui_tick: cml.ui_tick,
            player_pos: Vec3(player_pos),
            f_avatar_front: Vec3(cml.f_avatar_front.into()),
            cam_pos: Vec3(cam_pos),
            f_camera_front: Vec3(cml.f_camera_front.into()),
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

impl JokolayComponent for MumbleManager {
    fn flush_all_messages(&mut self) {
        let channels = self.channels.as_mut().unwrap();
        let mut messages = Vec::new();
        while let Ok(msg) = channels.notification_receiver.try_recv() {
            messages.push(from_data(msg));
        }
        for msg in messages {
            self.handle_message(msg);
        }
    }

    fn tick(&mut self, _latest_time: f64) -> ComponentDataExchange {
        let link = self._tick().unwrap_or(None);
        self.state.link = link.cloned();
        to_data(self.state.clone())
    }
    fn bind(
        &mut self,
        _deps: std::collections::HashMap<
            u32,
            tokio::sync::broadcast::Receiver<ComponentDataExchange>,
        >,
        mut bound: std::collections::HashMap<u32, PeerComponentChannel>, // ??? scsc if exists, this is a private channel only two bounded modules can use between each others.
        _input_notification: std::collections::HashMap<
            u32,
            tokio::sync::mpsc::Receiver<ComponentDataExchange>,
        >,
        _notify: std::collections::HashMap<u32, tokio::sync::mpsc::Sender<ComponentDataExchange>>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
    ) {
        let (notification_receiver, _) = bound.remove(&0).unwrap();
        let channels = MumbleChannels {
            notification_receiver,
        };
        self.channels = Some(channels);
    }
}

impl JokolayComponentDeps for MumbleManager {
    //default is enough
    fn peer(&self) -> Vec<&str> {
        if self.is_ui {
            vec!["back:mumble_link"]
        } else {
            vec!["ui:mumble_link"]
        }
    }
}
