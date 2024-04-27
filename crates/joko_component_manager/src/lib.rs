use std::collections::HashMap;

use joko_component_models::JokolayComponentDeps;

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

impl Default for ComponentManager {
    fn default() -> Self {
        Self::new()
    }
}
