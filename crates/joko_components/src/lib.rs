use std::collections::HashMap;

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

//could become a "dyn Message".
//std::any::Any is a trait
//TODO: It would have a wrap and unwrap ?
pub type ComponentDataExchange = Vec<u8>;
//pub type ComponentDataExchange = Box<[u8]>;
//pub type ComponentDataExchange = [u8; 1024];
pub type PeerComponentChannel = (
    tokio::sync::mpsc::Receiver<ComponentDataExchange>,
    tokio::sync::mpsc::Sender<ComponentDataExchange>,
);

pub trait JokolayComponent<SharedStatus, ComponentResult>
where
    SharedStatus: Clone,
{
    fn flush_all_messages(&mut self) -> SharedStatus;
    fn tick(&mut self, latest_time: f64) -> Option<&ComponentResult>;
    fn bind(
        &mut self,
        deps: HashMap<u32, tokio::sync::broadcast::Receiver<ComponentDataExchange>>,
        bound: HashMap<u32, PeerComponentChannel>, // ??? scsc if exists, this is a private channel only two bounded modules can use between each others.
        input_notification: HashMap<u32, tokio::sync::mpsc::Receiver<ComponentDataExchange>>,
        notify: HashMap<u32, tokio::sync::mpsc::Sender<ComponentDataExchange>>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
    ); //By default, there is no third party component, thus we can implement it as a noop

    /*

    // any extra information should come from configuration, which can be loaded from those two arguments.
    Those roots are specific to the component, it cannot shared it with another component
    pub fn new(
        root_dir: Arc<Dir>,
        root_path: &std::path::Path,
    ) -> Result<Self>;

    fn bind(
        &mut self,
        deps: HashMap<u32, tokio::sync::broadcast::receiver>,
        bound: HashMap<u32, tokio::sync::scsc::receiver +  sender>,// ??? scsc if exists, this is a private channel only two bounded modules can use between each others.
        input_notification: HashMap<u32, ???::receiver>
        notify: HashMap<u32, ???::sender>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
    )
    https://docs.rs/dep-graph/latest/dep_graph/
    https://lib.rs/crates/petgraph
    https://docs.rs/solvent/latest/solvent/
        => check "peer" is always mutual
        => graph with the "peer" elements replaced by some merged id
        => check there is no loop (there could be surprises)
        => if there is no problem, then:
            - build again the graph with UI plugins only and save one traversal (memory + file)
            - build again the graph with back plugins only and save one traversal (memory + file)
        => if there is a problem, do not save anything



    fn tick(
        &mut self,
    ) -> Option<&PluginResult>; where u32 is the position in bind() + requires()
    */
}

pub trait JokolayUIComponent<SharedStatus, ComponentResult>
where
    SharedStatus: Clone,
{
    fn flush_all_messages(&mut self) -> SharedStatus;
    fn tick(&mut self, latest_time: f64, egui_context: &egui::Context) -> Option<&ComponentResult>;
    fn bind(
        &mut self,
        deps: HashMap<u32, tokio::sync::broadcast::Receiver<ComponentDataExchange>>,
        bound: HashMap<u32, PeerComponentChannel>, // ??? scsc if exists, this is a private channel only two bounded modules can use between each others.
        input_notification: HashMap<u32, tokio::sync::mpsc::Receiver<ComponentDataExchange>>,
        notify: HashMap<u32, tokio::sync::mpsc::Sender<ComponentDataExchange>>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
    ); //By default, there is no third party component, thus we can implement it as a noop

    /*

    // any extra information should come from configuration, which can be loaded from those two arguments.
    Those roots are specific to the component, it cannot shared it with another component
    pub fn new(
        root_dir: Arc<Dir>,
        root_path: &std::path::Path,
    ) -> Result<Self>;

    fn bind(
        &mut self,
        deps: HashMap<u32, tokio::sync::broadcast::receiver>,
        bound: HashMap<u32, tokio::sync::scsc::receiver +  sender>,// ??? scsc if exists, this is a private channel only two bounded modules can use between each others.
        input_notification: HashMap<u32, ???::receiver>
        notify: HashMap<u32, ???::sender>, // used to send a message to another plugin. This is a reversed requirement. A plugin force itself into the path of another.
    )
    https://docs.rs/dep-graph/latest/dep_graph/
    https://lib.rs/crates/petgraph
    https://docs.rs/solvent/latest/solvent/
        => check "peer" is always mutual
        => graph with the "peer" elements replaced by some merged id
        => check there is no loop (there could be surprises)
        => if there is no problem, then:
            - build again the graph with UI plugins only and save one traversal (memory + file)
            - build again the graph with back plugins only and save one traversal (memory + file)
        => if there is a problem, do not save anything



    fn tick(
        &mut self,
    ) -> Option<&PluginResult>; where u32 is the position in bind() + requires()
    */
}

//TODO: have a BackEndPlugin and UIPlugin

pub struct ComponentManager {
    data: HashMap<String, Box<dyn JokolayComponentDeps>>,
}

impl ComponentManager {
    pub fn new() -> Self {
        Self {
            data: Default::default(),
        }
    }

    pub fn register(&mut self, service_name: &str, co: Box<dyn JokolayComponentDeps>) {
        self.data.insert(service_name.to_owned(), co);
    }

    pub fn build_routes(&mut self) -> Result<(), String> {
        let mut known_services: HashMap<String, u32> = Default::default();
        let mut service_id = 0;
        for (service_name, co) in self.data.iter() {
            service_id += 1;
            known_services.insert(service_name.clone(), service_id);
            for peer_name in co.peer() {
                if let Some(peer) = self.data.get(peer_name) {
                    if !peer.peer().contains(&service_name.as_str()) {
                        return Err(format!(
                            "Missmatch in peer between {} and {}",
                            service_name, peer_name
                        ));
                    }
                }
            }
        }
        unimplemented!(
            "The algorithm to build and check dependancies between components is not implemented"
        )
    }
}
