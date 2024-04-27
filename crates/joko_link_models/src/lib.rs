//! Jokolink is a crate to deal with Mumble Link data exposed by games/apps on windows via shared memory

//! Joko link is designed to primarily get the MumbleLink or the window size
//! of the GW2 window for Jokolay (an crossplatform overlay for Guild Wars 2).
//! on windows, you can use it to create/open shared memory.
//! and on linux, you can run jokolink binary in wine, which will create/open shared memory and copy-paste it into /dev/shm.
//! then, you can easily read the /dev/shm file from a any number of linux native applications.
//! along with mumblelink data, it also copies the x11 window id of gw2. you can use this to get the size of gw2 window.
//!

mod mumble;

pub use mumble::*;

pub enum MessageToMumbleLinkBack {
    BindedOnUI,
    Autonomous,
    Value(Option<MumbleLink>), //pushed from a value imposed by UI. Either a form or a traveling for demo.
}

#[derive(Clone)]
pub struct MumbleLinkSharedState {
    pub read_ui_link: bool,
    pub copy_of_ui_link: Option<MumbleLink>,
}
