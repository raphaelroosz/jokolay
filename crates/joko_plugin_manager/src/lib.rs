use joko_component_models::{
    default_data_exchange, ComponentDataExchange, JokolayComponent, JokolayComponentDeps,
    PeerComponentChannel,
};

pub struct JokolayPlugin {}

pub struct JokolayPluginManager {}

impl JokolayComponent for JokolayPlugin {
    fn flush_all_messages(&mut self) {}
    fn tick(&mut self, _timestamp: f64) -> ComponentDataExchange {
        default_data_exchange()
    }
    fn bind(
        &mut self,
        _deps: std::collections::HashMap<
            u32,
            tokio::sync::broadcast::Receiver<ComponentDataExchange>,
        >,
        _bound: std::collections::HashMap<u32, PeerComponentChannel>, // ??? scsc if exists, this is a private channel only two bounded modules can use between each others.
        _input_notification: std::collections::HashMap<
            u32,
            tokio::sync::mpsc::Receiver<ComponentDataExchange>,
        >,
        _notify: std::collections::HashMap<u32, tokio::sync::mpsc::Sender<ComponentDataExchange>>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
    ) {
    }
}
impl JokolayComponentDeps for JokolayPlugin {
    fn requires(&self) -> Vec<&str> {
        vec!["back:mumble_link"]
    }
}
