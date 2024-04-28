use std::collections::HashMap;

#[cfg(feature = "messages_any")]
mod messages_any;
#[cfg(feature = "messages_any")]
pub use messages_any::*;

#[cfg(feature = "messages_bincode")]
mod messages_bincode;
#[cfg(feature = "messages_bincode")]
pub use messages_bincode::*;

pub type PeerComponentChannel = (
    tokio::sync::mpsc::Receiver<ComponentDataExchange>,
    tokio::sync::mpsc::Sender<ComponentDataExchange>,
);

pub trait JokolayComponentDeps {
    /**
    Names are external to traits and implementation. That way it is easy to change it without change in binary.
    In case of first class components, name is hardcoded.
    In case of plugins, name is part of a manifest and can be changed at will.
    */
    // elements in peer(), requires() and notify() are mutually exclusives
    fn peer(&self) -> Vec<&str> {
        //by default, no other plugin bound
        vec![]
    }
    fn requires(&self) -> Vec<&str> {
        //by default, no requirement
        vec![]
    }
    fn notify(&self) -> Vec<&str> {
        //by default, no third party plugin
        vec![]
    }
}

pub trait JokolayComponent {
    /*
    This make sense only when components are very similar. It make no sense to ask for a uniform way to build components.
    type T;
    type E;
    fn new(
        root_path: &std::path::Path,
    ) -> Result<Self::T, Self::E>;*/

    fn flush_all_messages(&mut self);
    fn tick(&mut self, latest_time: f64) -> ComponentDataExchange;
    fn bind(
        &mut self,
        deps: HashMap<u32, tokio::sync::broadcast::Receiver<ComponentDataExchange>>,
        bound: HashMap<u32, PeerComponentChannel>, // Private channel only two bounded modules can use between each others.
        input_notification: HashMap<u32, tokio::sync::mpsc::Receiver<ComponentDataExchange>>,
        notify: HashMap<u32, tokio::sync::mpsc::Sender<ComponentDataExchange>>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
    ); //By default, there is no third party component, thus we can implement it as a noop
       /*
           TODO: there could be an optional trait: Chain.
           If there is a strong connection between two elements, passing values by channels and copy could be inefficient, calling a function with arguments could be better =>
               it's almost a macro with an unset number of arguments and unknown types.
               It could be possible on plugins, not other kind of components
       */
}

pub trait JokolayUIComponent<ComponentResult>
where
    ComponentResult: Clone,
{
    fn flush_all_messages(&mut self);
    //the only reason there is another Component trait is because of the egui_context
    fn tick(&mut self, latest_time: f64, egui_context: &egui::Context) -> ComponentResult;
    fn bind(
        &mut self,
        deps: HashMap<u32, tokio::sync::broadcast::Receiver<ComponentDataExchange>>,
        bound: HashMap<u32, PeerComponentChannel>, // Private channel only two bounded modules can use between each others.
        input_notification: HashMap<u32, tokio::sync::mpsc::Receiver<ComponentDataExchange>>,
        notify: HashMap<u32, tokio::sync::mpsc::Sender<ComponentDataExchange>>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
    ); //By default, there is no third party component, thus we can implement it as a noop
}
