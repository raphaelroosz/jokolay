use std::collections::HashMap;

use joko_component_models::JokolayComponentDeps;
use petgraph::{
    csr::IndexType, graph::NodeIndex, stable_graph::StableDiGraph, visit::IntoNodeIdentifiers,
    Direction,
};
use tracing::trace;

pub struct ComponentManager {
    data: HashMap<String, Box<dyn JokolayComponentDeps>>,
}

fn get_invocation_order<N, E, Ix>(my_graph: &mut StableDiGraph<N, E, Ix>) -> Vec<N>
where
    N: std::cmp::Ord,
    Ix: IndexType,
{
    let mut invocation_order = Vec::new();

    //peel nodes one by one
    while my_graph.externals(Direction::Outgoing).count() > 0 {
        let mut to_delete = Vec::new();
        for external_node in my_graph.externals(Direction::Outgoing) {
            to_delete.push(external_node);
        }
        let mut current_level_invocation_order = Vec::new();
        for external_node in to_delete {
            current_level_invocation_order.push(my_graph.remove_node(external_node).unwrap());
        }
        current_level_invocation_order.sort(); //This grant a deterministic order regardless of circumstances
        invocation_order.extend(current_level_invocation_order);
    }
    //if there is a cycle, there are remaining nodes
    invocation_order
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
        /*

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

        type G = petgraph::stable_graph::StableDiGraph<u32, u32, u16>;

        let mut known_services: HashMap<String, NodeIndex<u16>> = Default::default();
        let mut depgraph: G = G::default();
        let mut translation: HashMap<NodeIndex<u16>, NodeIndex<u16>> = Default::default();
        let mut service_id = 0;
        for (service_name, co) in self.data.iter() {
            let service_name = service_name.clone();
            if !known_services.contains_key(&service_name) {
                let node_id = depgraph.add_node(service_id);
                service_id += 1;
                known_services.insert(service_name.clone(), node_id);
            }
            trace!("node: {}, peers: {:?}", service_name, co.peer());
            for peer_name in co.peer() {
                let peer_name = peer_name.to_string();
                if !known_services.contains_key(&peer_name) {
                    let node_id = depgraph.add_node(service_id);
                    service_id += 1;
                    known_services.insert(peer_name.clone(), node_id);
                }
                if let Some(peer) = self.data.get(&peer_name) {
                    if !peer.peer().contains(&service_name.as_str()) {
                        return Err(format!(
                            "Missmatch in peers: '{}' asked for '{}' to be a peer, reverse is not true",
                            service_name, peer_name
                        ));
                    }
                    let parent_id = *known_services.get(&service_name).unwrap();
                    let peer_id = *known_services.get(&peer_name).unwrap();
                    let merged_id = parent_id.min(peer_id);
                    translation.insert(parent_id, merged_id);
                    translation.insert(peer_id, merged_id);
                }
            }
        }
        //If we reached here, it means all peers agree

        let mut requirements_graph = depgraph.clone();
        let mut notification_graph = depgraph.clone();

        for (service_name, co) in self.data.iter() {
            let node_id = *known_services.get(service_name).unwrap();
            let service_id = *translation.get(&node_id).or(Some(&node_id)).unwrap();
            trace!("node: {}, requires: {:?}", service_name, co.requires());
            for required_service_name in co.requires() {
                let required_service_id = *known_services.get(required_service_name).unwrap();
                let required_service_id = *translation
                    .get(&required_service_id)
                    .or(Some(&required_service_id))
                    .unwrap();
                if service_id != required_service_id {
                    depgraph.add_edge(service_id, required_service_id, 1);
                    //The ids are improper since coming from the other graph. But both graphs are clones so it should be fine.
                    requirements_graph.add_edge(service_id, required_service_id, 1);
                }
            }
            trace!("node: {}, notify: {:?}", service_name, co.notify());
            for notified_service_name in co.notify() {
                let notified_service_id = *known_services.get(notified_service_name).unwrap();
                let notified_service_id = *translation
                    .get(&notified_service_id)
                    .or(Some(&notified_service_id))
                    .unwrap();
                if service_id != notified_service_id {
                    depgraph.add_edge(notified_service_id, service_id, 1);
                    //The ids are improper since coming from the other graph. But both graphs are clones so it should be fine.
                    notification_graph.add_edge(notified_service_id, service_id, 1);
                }
            }
        }

        let invocation_order = get_invocation_order(&mut depgraph);
        if depgraph.node_count() > 0 {
            return Err(format!(
                "Found a cyclic dependancy between {:?}",
                depgraph.node_identifiers()
            ));
        }
        trace!("services: {:?}", known_services);
        trace!("invocation_order: {:?}", invocation_order);
        /*
        TODO: make use of:
            requirements graph
            notification graph
            invocation order
        */

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

#[cfg(test)]
mod test {
    #[test]
    fn test_invocation_order_1() {
        type G = petgraph::stable_graph::StableDiGraph<String, u32, u16>;
        let mut my_graph = G::default();
        let a = my_graph.add_node("a".to_string());
        let b = my_graph.add_node("b".to_string());
        let c = my_graph.add_node("c".to_string());
        let d = my_graph.add_node("d".to_string());
        let _e = my_graph.add_node("e".to_string());

        my_graph.add_edge(b, c, 1);
        my_graph.add_edge(a, c, 1);
        my_graph.add_edge(c, d, 1);
        my_graph.add_edge(a, d, 1);

        println!("nb nodes: {}", my_graph.node_count());
        let invocation_order = crate::get_invocation_order(&mut my_graph);
        println!("nb nodes: {}", my_graph.node_count());
        println!("invocation order: {:?}", invocation_order);
        assert!(my_graph.node_count() == 0);
    }

    #[test]
    fn test_invocation_order_2() {
        type G = petgraph::stable_graph::StableDiGraph<String, u32, u16>;
        let mut my_graph = G::default();
        let a = my_graph.add_node("a".to_string());
        let b = my_graph.add_node("b".to_string());
        let c = my_graph.add_node("c".to_string());

        my_graph.add_edge(a, b, 1);
        my_graph.add_edge(b, a, 1);
        my_graph.add_edge(b, c, 1);

        println!("nb nodes: {}", my_graph.node_count());
        let invocation_order = crate::get_invocation_order(&mut my_graph);
        println!("nb nodes: {}", my_graph.node_count());
        println!("invocation order: {:?}", invocation_order);
        assert!(my_graph.node_count() == 2);
    }

    #[test]
    fn test_invocation_order_3() {
        type GG = petgraph::stable_graph::StableDiGraph<u32, u32, u16>;
        let mut my_graph = GG::default();
        let a = my_graph.add_node(1);
        let b = my_graph.add_node(2);
        let c = my_graph.add_node(3);

        my_graph.add_edge(a, b, 1);
        my_graph.add_edge(b, a, 1);
        my_graph.add_edge(b, c, 1);

        println!("nb nodes: {}", my_graph.node_count());
        let invocation_order = crate::get_invocation_order(&mut my_graph);
        println!("nb nodes: {}", my_graph.node_count());
        println!("invocation order: {:?}", invocation_order);
        assert!(my_graph.node_count() == 2);
    }

    #[test]
    fn test_invocation_order_4() {
        type GG = petgraph::stable_graph::StableDiGraph<u32, u32, u16>;
        let mut my_graph = GG::default();
        let a = my_graph.add_node(1);
        let b = my_graph.add_node(2);
        let c = my_graph.add_node(3);

        my_graph.add_edge(a, b, 1);
        my_graph.add_edge(b, c, 1);
        my_graph.add_edge(a, c, 1);

        println!("nb nodes: {}", my_graph.node_count());
        let invocation_order = crate::get_invocation_order(&mut my_graph);
        println!("nb nodes: {}", my_graph.node_count());
        println!("invocation order: {:?}", invocation_order);
        assert!(my_graph.node_count() == 0);
    }

    #[test]
    fn test_duplicate_node_value() {
        type GG = petgraph::stable_graph::StableDiGraph<u32, u32, u16>;
        let mut my_graph = GG::default();
        let a = my_graph.add_node(1);
        let b = my_graph.add_node(2);
        let c = my_graph.add_node(3);
        let _doublon = my_graph.add_node(3); // same value, considered as a separate node

        my_graph.add_edge(a, b, 1);
        my_graph.add_edge(b, a, 1);
        my_graph.add_edge(a, c, 1);

        println!("nb nodes: {}", my_graph.node_count());
        let invocation_order = crate::get_invocation_order(&mut my_graph);
        println!("nb nodes: {}", my_graph.node_count());
        println!("invocation order: {:?}", invocation_order);
        assert!(my_graph.node_count() == 2);
    }
}
