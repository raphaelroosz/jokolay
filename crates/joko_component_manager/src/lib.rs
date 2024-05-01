use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

use joko_component_models::{ComponentChannels, ComponentDataExchange, JokolayComponent};
use petgraph::{
    csr::IndexType,
    graph::NodeIndex,
    stable_graph::{EdgeReference, StableDiGraph},
    visit::{EdgeRef, IntoNodeIdentifiers},
};
use tracing::trace;

type BroadcastChannels = (
    tokio::sync::broadcast::Sender<ComponentDataExchange>,
    tokio::sync::broadcast::Receiver<ComponentDataExchange>,
);
pub struct ComponentManager {
    //TODO: make it a component too ?
    known_components: HashMap<String, ComponentHandle>,
    broadcasters: HashMap<String, BroadcastChannels>, //a receiver is kept idle in order to not close the channels. https://docs.rs/tokio/latest/tokio/sync/broadcast/#closing
    notifications: HashMap<String, tokio::sync::mpsc::Sender<ComponentDataExchange>>,
}

struct ComponentHandle {
    name: String,
    component: Box<dyn JokolayComponent>,
    channels: ComponentChannels,
    relations_to_ids: HashMap<String, usize>,
}
pub struct ComponentExecutor {
    components: Vec<ComponentHandle>, //FIXME: how to type erase result ?
}

#[derive(Clone)]
enum RelationShip {
    Requires,
    Peer,
    Notify,
}
fn get_invocation_order<N, E, Ix, F>(
    graph: &mut StableDiGraph<N, E, Ix>,
    filter: F,
) -> Vec<NodeIndex<Ix>>
where
    N: std::cmp::Ord,
    Ix: IndexType,
    F: Fn(EdgeReference<E, Ix>) -> bool,
{
    let mut invocation_order = Vec::new();

    //peel nodes one by one
    let mut modified = true;
    while modified {
        modified = false;
        let mut to_delete = Vec::new();
        for node_id in graph.node_indices() {
            let mut is_pointed = false;
            for edge in graph.edges(node_id) {
                if filter(edge) {
                    is_pointed = true;
                    break;
                }
            }
            if !is_pointed {
                to_delete.push(node_id);
            }
        }
        let mut current_level_invocation_order = Vec::new();
        for external_node in to_delete {
            graph.remove_node(external_node);
            modified = true;
            current_level_invocation_order.push(external_node);
        }
        current_level_invocation_order.sort(); //This grant a deterministic order regardless of circumstances
        invocation_order.extend(current_level_invocation_order);
    }

    //if there is a cycle, there are remaining nodes
    invocation_order
}

fn has_unique_elements<T>(iter: T) -> bool
where
    T: IntoIterator,
    T::Item: Eq + Hash,
{
    let mut uniq = HashSet::new();
    iter.into_iter().all(move |x| uniq.insert(x))
}
impl ComponentManager {
    pub fn new() -> Self {
        //clone itself on a world basis ? which would follow a component thread
        Self {
            known_components: Default::default(),
            broadcasters: Default::default(),
            notifications: Default::default(),
        }
    }

    /// Register a component.
    /// On its relationship, each component reference (names) shall be assigned an id.
    /// That id is 0 based and goes in following order: peers, notify, requirements
    /// A component, when binding must retrieve the with the proper id.
    pub fn register(
        &mut self,
        component_name: &str,
        component: Box<dyn JokolayComponent>,
    ) -> Result<(), String> {
        if !has_unique_elements(
            component
                .peers()
                .iter()
                .chain(component.notify().iter())
                .chain(component.requirements().iter()),
        ) {
            return Err(format!(
                "Service {} has duplicate elements. Each name can only appear at one place",
                component_name
            ));
        }
        let mut relations_to_ids: HashMap<String, usize> = Default::default();
        for (idx, name) in component
            .peers()
            .iter()
            .chain(component.notify().iter())
            .chain(component.requirements().iter())
            .enumerate()
        {
            relations_to_ids.insert(name.to_string(), idx);
        }

        let handle = ComponentHandle {
            name: component_name.to_string(),
            component,
            channels: ComponentChannels::default(),
            relations_to_ids,
        };
        self.known_components
            .insert(component_name.to_string(), handle);

        Ok(())
    }

    pub fn executor(&self, world: &str) -> ComponentExecutor {
        /*
        TODO:
            extract the list of components of this world
            bind them
            insert them into the executor
        */
        ComponentExecutor {
            components: Default::default(),
        }
    }

    /// Check, create and bind the relationships and communication channels between the components.
    /// A world define what is accessible. Mostly for execution separation purpose (another thread, server, anything).
    /// "requirements" must be a DAG. It must always be in the same "world".
    /// "peers" must be mutual.
    ///
    pub fn build_routes(&mut self) -> Result<(), String> {
        //TODO: check worlds

        type G = petgraph::stable_graph::StableDiGraph<String, RelationShip, u16>;

        //TODO: those are temporary channels, one should work between existing and new channels => we need to save the work of a previous build
        let mut notifications: HashMap<String, tokio::sync::mpsc::Sender<ComponentDataExchange>> =
            Default::default();
        let mut broadcasters: HashMap<String, BroadcastChannels> = Default::default();

        let mut known_services: bimap::BiHashMap<String, NodeIndex<u16>> = Default::default();
        let mut depgraph: G = G::default();

        // initialize the basic channels
        for (component_name, handle) in self.known_components.iter_mut() {
            let node_id = depgraph.add_node(component_name.clone());
            known_services.insert(component_name.clone(), node_id);
            if handle.component.accept_notifications() {
                let (sender, receiver) = tokio::sync::mpsc::channel(1000);
                handle.channels.input_notification = Some(receiver);
                notifications.insert(component_name.clone(), sender);
            }
            broadcasters.insert(component_name.clone(), tokio::sync::broadcast::channel(1));
        }

        // register nodes
        for handle in self.known_components.values() {
            let component = &handle.component;
            for peer_name in component.peers() {
                let peer_name = peer_name.to_string();
                if !known_services.contains_left(&peer_name) {
                    let node_id = depgraph.add_node(peer_name.clone());
                    known_services.insert(peer_name.clone(), node_id);
                }
            }
            for required_service_name in component.requirements() {
                let required_service_name = required_service_name.to_string();
                if !known_services.contains_left(&required_service_name) {
                    let node_id = depgraph.add_node(required_service_name.clone());
                    known_services.insert(required_service_name.clone(), node_id);
                }
            }
            for notified_service_name in component.notify() {
                let notified_service_name = notified_service_name.to_string();
                if !known_services.contains_left(&notified_service_name) {
                    let node_id = depgraph.add_node(notified_service_name.clone());
                    known_services.insert(notified_service_name.clone(), node_id);
                }
            }
        }

        // register relationships
        for (component_name, handle) in self.known_components.iter() {
            let component = &handle.component;
            let node_id = *known_services.get_by_left(component_name).unwrap();

            trace!("node: {}, peers: {:?}", component_name, component.peers());
            for peer_name in component.peers() {
                let peer_name = peer_name.to_string();
                if let Some(peer_handle) = self.known_components.get(&peer_name) {
                    let peer = &peer_handle.component;
                    trace!("peer: {}, peers: {:?}", peer_name, peer.peers());
                    if !peer.peers().contains(&component_name.as_str()) {
                        return Err(format!(
                            "Missmatch in peers: '{}' asked for '{}' to be a peer, reverse is not true",
                            component_name, peer_name
                        ));
                    }
                    let peer_id = *known_services.get_by_left(&peer_name).unwrap();
                    let mut has_rel = false;
                    for e in depgraph.edges_connecting(node_id, peer_id) {
                        if let RelationShip::Peer = e.weight() {
                            has_rel = true;
                            break;
                        }
                    }
                    if !has_rel {
                        depgraph.add_edge(node_id, peer_id, RelationShip::Peer);
                        depgraph.add_edge(peer_id, node_id, RelationShip::Peer);
                    }
                }
            }
            trace!(
                "node: {}, requires: {:?}",
                component_name,
                component.requirements()
            );
            for required_service_name in component.requirements() {
                let required_service_id =
                    *known_services.get_by_left(required_service_name).unwrap();
                //let required_service_id = *translation.get(&required_service_id).unwrap_or(&required_service_id);
                if node_id != required_service_id {
                    depgraph.add_edge(node_id, required_service_id, RelationShip::Requires);
                    //The ids are improper since coming from the other graph. But both graphs are clones so it should be fine.
                }
            }
            trace!("node: {}, notify: {:?}", component_name, component.notify());
            for notified_service_name in component.notify() {
                let notified_service_id =
                    *known_services.get_by_left(notified_service_name).unwrap();
                //let notified_service_id = *translation.get(&notified_service_id).unwrap_or(&notified_service_id);
                if node_id != notified_service_id {
                    //there is no dep on the graph, the only worth of the notified service is it needs to exist
                    //The ids are improper since coming from the other graph. But both graphs are clones so it should be fine.
                    depgraph.add_edge(node_id, notified_service_id, RelationShip::Notify);
                }
            }
        }
        //If we reached here, it means all peers agree.

        //Is there a difference between keys of known_services vs hosted_services.
        let hosted_keys: HashSet<String> = self.known_components.keys().cloned().collect();
        let known_keys: HashSet<String> = depgraph.node_weights().cloned().collect();
        trace!("hosted_keys: {:?}", hosted_keys);
        trace!("known_keys: {:?}", known_keys);
        if known_keys.difference(&hosted_keys).count() > 0 {
            //TODO: have error!() with details of which component asked for it
            return Err(format!(
                "Some relationship could not be satisfied. Missing: {:?}",
                known_keys.difference(&hosted_keys)
            ));
        }
        // no missing component

        // check for cycles
        let mut graph_copy = depgraph.clone();
        let invocation_order = get_invocation_order(&mut graph_copy, |e| matches!(e.weight(), RelationShip::Requires));
        if graph_copy.node_count() > 0 {
            return Err(format!(
                "Found a cyclic dependancy between {:?}",
                graph_copy.node_identifiers()
            ));
        }
        // no cycle

        trace!("services: {:?}", known_services);
        trace!("invocation_order: {:?}", invocation_order);

        // At this point, every relationship is sane, none missing, no cycle. We can now build the communication channels.

        /*
        TODO: make use of:
            requirements graph => components subscribe to it. There should be at most one element in it, eaten at each step of the loop.
                => how to make sure ui does subscribe to ui only and back to back ? => introduce "worlds" "myworld:component"
            notification graph
            invocation order
        */

        //TODO: channels are part of each component handle, all that remains is insert them
        /*for (node_id, _) in translation.iter() {
            peers_channels.insert(node_id.clone(), tokio::sync::mpsc::channel(1000));
        }*/
        for node_id in depgraph.node_indices() {
            let notify_rel = depgraph.edges(node_id).filter(|e| matches!(e.weight(), RelationShip::Notify));
            for rel in notify_rel {
                let dst_node_id = rel.target();
                let dst_component_name = known_services.get_by_right(&dst_node_id).unwrap();
                if let Some(sender) = notifications.get(dst_component_name) {
                    let src_node_id = rel.source();
                    let src_component_name = known_services.get_by_right(&src_node_id).unwrap();
                    let src_handle = self.known_components.get_mut(src_component_name).unwrap();
                    trace!(
                        "{} wants to notify {}",
                        src_component_name,
                        dst_component_name
                    );
                    trace!("source map: {:?}", src_handle.relations_to_ids);
                    let dst_relative_id =
                        *src_handle.relations_to_ids.get(dst_component_name).unwrap();
                    if let Some(src_component) = self.known_components.get_mut(src_component_name) {
                        src_component
                            .channels
                            .notify
                            .insert(dst_relative_id, sender.clone());
                    }
                }
            }
            let peer_rel = depgraph.edges(node_id).filter(|e| matches!(e.weight(), RelationShip::Peer));
            for rel in peer_rel {
                // we shall overwrite the channels, but this is ok since we are not using them yet.
                // TODO: if in the future there is dynamic loading, there shall be a need to dynamically rebuilt and thus get and reuse the existing channels.
                let (local, remote) = {
                    let (sender_1, receiver_1) = tokio::sync::mpsc::channel(1000);
                    let (sender_2, receiver_2) = tokio::sync::mpsc::channel(1000);
                    ((sender_1, receiver_2), (sender_2, receiver_1))
                };
                let src_node_id = rel.source();
                let src_component_name = known_services.get_by_right(&src_node_id).unwrap();
                let dst_node_id = rel.target();
                let dst_component_name = known_services.get_by_right(&dst_node_id).unwrap();

                let src_handle = self.known_components.get_mut(src_component_name).unwrap();
                let dst_relative_id = *src_handle.relations_to_ids.get(dst_component_name).unwrap();
                src_handle.channels.peers.insert(dst_relative_id, local);

                let dst_handle = self.known_components.get_mut(dst_component_name).unwrap();
                let src_relative_id = *dst_handle.relations_to_ids.get(src_component_name).unwrap();
                dst_handle.channels.peers.insert(src_relative_id, remote);
            }

            let requirement_rel = depgraph.edges(node_id).filter(|e| matches!(e.weight(), RelationShip::Requires));
            for rel in requirement_rel {
                let src_node_id = rel.source();
                let src_component_name = known_services.get_by_right(&src_node_id).unwrap();
                let dst_node_id = rel.target();
                let dst_component_name = known_services.get_by_right(&dst_node_id).unwrap();

                let src_handle = self.known_components.get_mut(src_component_name).unwrap();
                let dst_relative_id = *src_handle.relations_to_ids.get(dst_component_name).unwrap();
                let (sender, _) = broadcasters.get(src_component_name).unwrap();
                src_handle
                    .channels
                    .requirements
                    .insert(dst_relative_id, sender.subscribe());
            }
        }

        for (service_name, handle) in self.known_components.iter_mut() {
            trace!(
                "bind {} with, notified: {}, notify: {}, requirements: {}, peers: {}",
                service_name,
                handle.channels.input_notification.is_some(),
                handle.channels.notify.len(),
                handle.channels.requirements.len(),
                handle.channels.peers.len(),
            );
            trace!("Component ids: {:?}", handle.relations_to_ids);
            handle.component.bind(std::mem::take(&mut handle.channels));
        }

        //unimplemented!("The algorithm to build and check dependancies between components is not implemented");
        self.broadcasters = broadcasters;
        self.notifications = notifications;
        Ok(())
    }
}

impl Default for ComponentManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentHandle {
    fn broadcast(&mut self, data: ComponentDataExchange) {
        println!("{:?}", data);
        unimplemented!("The broadcast of data is not implemented");
    }
}

impl ComponentExecutor {
    fn tick(&mut self, latest_time: f64) {
        for handle in self.components.iter_mut() {
            let res = handle.component.tick(latest_time);
            handle.broadcast(res);
        }
        unimplemented!("The component executor tick is not implemented");
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
        let invocation_order = crate::get_invocation_order(&mut my_graph, |_e| true);
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
        let invocation_order = crate::get_invocation_order(&mut my_graph, |_e| true);
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
        let invocation_order = crate::get_invocation_order(&mut my_graph, |_e| true);
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
        let invocation_order = crate::get_invocation_order(&mut my_graph, |_e| true);
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
        let invocation_order = crate::get_invocation_order(&mut my_graph, |_e| true);
        println!("nb nodes: {}", my_graph.node_count());
        println!("invocation order: {:?}", invocation_order);
        assert!(my_graph.node_count() == 2);
    }
}
