#[cfg(feature = "messages_any")]
mod messages_any;
#[cfg(feature = "messages_any")]
pub use messages_any::*;

#[cfg(feature = "messages_bincode")]
mod messages_bincode;
#[cfg(feature = "messages_bincode")]
pub use messages_bincode::*;

pub type PeerComponentChannel = (
    tokio::sync::mpsc::Sender<ComponentMessage>,
    tokio::sync::mpsc::Receiver<ComponentMessage>,
);

pub trait Component: Send + Sync {
    /**
    Names are external to traits and implementation. That way it is easy to change it without change in binary.
    In case of first class components, name is hardcoded.
    In case of plugins, name is part of a manifest and can be changed at will.
    */
    //TODO: fn watch(&self) -> Vec<&str> {}
    // elements in peer(), requires() and notify() are mutually exclusives
    fn peers(&self) -> Vec<&str> {
        //by default, no other plugin bound
        vec![]
    }
    /// Shall eat a new value produced by the required components at each tick
    /// By default, no requirement
    fn requirements(&self) -> Vec<&str> {
        vec![]
    }
    fn notify(&self) -> Vec<&str> {
        //by default, no third party plugin
        vec![]
    }
    fn accept_notifications(&self) -> bool {
        false
    }
    /*
    TODO:
        for global values that does not need a specific new value at each frame (such as configuration), watch over the values.
        fn watch(&self) -> Vec<&str>
        https://docs.rs/tokio/latest/tokio/sync/watch/index.html
    */

    /// called once after building relationships
    fn init(&mut self);

    /// Drain every notifications sent by any other component
    fn flush_all_messages(&mut self);

    fn tick(&mut self, latest_time: f64) -> ComponentResult;

    /// when reasing the channels, the id of channels are set by their appearance order in "peers", then "requirements", then "notify"
    fn bind(&mut self, channels: ComponentChannels);
    /*
        TODO: there could be an optional trait: Chain.
        If there is a strong connection between two elements, passing values by channels and copy could be inefficient, calling a function with arguments could be better =>
            it's almost a macro with an unset number of arguments and unknown types.
            It could be possible on plugins, not other kind of components
    */
}

/// when reasing the channels, the id of channels are set by their appearance order in "peers", then "requirements", then "notify"
#[derive(Default)]
pub struct ComponentChannels {
    pub requirements:
        std::collections::HashMap<usize, tokio::sync::broadcast::Receiver<ComponentResult>>,
    pub peers: std::collections::HashMap<usize, PeerComponentChannel>, // ??? scsc if exists, this is a private channel only two bounded modules can use between each others.
    pub input_notification: Option<tokio::sync::mpsc::Receiver<ComponentMessage>>,
    pub notify: std::collections::HashMap<usize, tokio::sync::mpsc::Sender<ComponentMessage>>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
}
