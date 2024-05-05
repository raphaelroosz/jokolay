use crate::{
    manager::{LoadedPackData, LoadedPackTexture},
    BASE64_ENGINE,
};
use base64::Engine;
use glam::Vec3;
use joko_package_models::{
    attributes::XotAttributeNameIDs, category::Category, marker::Marker, route::Route, trail::Trail,
};
use miette::Result;
use ordered_hash_map::OrderedHashMap;
use std::{io::Write, path::Path};
use tracing::info;
use uuid::Uuid;
use xot::{Element, Node, SerializeOptions, Xot};

/// Save the pack core as xml pack using the given directory as pack root path.
pub(crate) fn save_pack_data_to_dir(
    pack_data: &LoadedPackData,
    writing_directory: &Path,
) -> Result<(), String> {
    // save categories
    info!(
        "Saving data pack {}, {} topmost categories, {} maps into {:?}",
        pack_data.name,
        pack_data.categories.len(),
        pack_data.maps.len(),
        writing_directory
    );
    std::fs::create_dir_all(writing_directory).or(Err("failed to create core pack directory"))?;
    let mut tree = Xot::new();
    let names = XotAttributeNameIDs::register_with_xot(&mut tree);
    let od = tree.new_element(names.overlay_data);
    let root_node = tree
        .new_root(od)
        .or(Err("failed to create new root with overlay data node"))?;
    recursive_cat_serializer(&mut tree, &names, &pack_data.categories, od)?;
    let cats = tree
        .with_serialize_options(SerializeOptions { pretty: true })
        .to_string(root_node)
        .or(Err("failed to convert cats xot to string"))?;

    let target = writing_directory.join("categories.xml");
    std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(target)
        .or(Err("failed to create categories.xml"))?
        .write_all(cats.as_bytes())
        .or(Err("failed to write to categories.xml"))?;

    // save maps
    for (map_id, map_data) in pack_data.maps.iter() {
        if map_data.markers.is_empty() && map_data.trails.is_empty() {
            let map_file_name = writing_directory.join(format!("{map_id}.xml"));
            if let Err(e) = std::fs::remove_file(map_file_name) {
                info!(
                    ?e,
                    map_id, "failed to remove xml file that had nothing to write to"
                );
            }
        }
        let mut tree = Xot::new();
        let names = XotAttributeNameIDs::register_with_xot(&mut tree);
        let od = tree.new_element(names.overlay_data);
        let root_node: Node = tree
            .new_root(od)
            .or(Err("failed to create root wiht overlay data for pois"))?;
        let pois = tree.new_element(names.pois);
        tree.append(od, pois)
            .or(Err("faild to append pois to od node"))?;
        for marker in map_data.markers.values() {
            let poi = tree.new_element(names.poi);
            tree.append(pois, poi)
                .or(Err("failed to append poi (marker) to pois"))?;
            let ele = tree.element_mut(poi).unwrap();
            serialize_marker_to_element(marker, ele, &names);
        }
        for route_path in map_data.routes.values() {
            serialize_route_to_element(&mut tree, route_path, &pois, &names)?;
        }
        for trail in map_data.trails.values() {
            if trail.dynamic {
                continue;
            }
            let trail_node = tree.new_element(names.trail);
            tree.append(pois, trail_node)
                .or(Err("failed to append a trail node to pois"))?;
            let ele = tree.element_mut(trail_node).unwrap();
            serialize_trail_to_element(trail, ele, &names);
        }
        let map_xml = tree
            .with_serialize_options(SerializeOptions { pretty: true })
            .to_string(root_node)
            .or(Err("failed to serialize map data to string"))?;
        let target = writing_directory.join(format!("{map_id}.xml"));
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(target)
            .or(Err("failed to create map xml file"))?
            .write_all(map_xml.as_bytes())
            .or(Err("failed to write map data to file"))?;
    }
    Ok(())
}
pub(crate) fn save_pack_texture_to_dir(
    pack_texture: &LoadedPackTexture,
    writing_directory: &Path,
) -> Result<(), String> {
    info!(
        "Saving texture pack {}, {} textures, {} tbins",
        pack_texture.name,
        pack_texture.textures.len(),
        pack_texture.tbins.len()
    );
    std::fs::create_dir_all(writing_directory).or(Err("failed to create core pack directory"))?;
    // save images
    for (img_path, img) in pack_texture.textures.iter() {
        if let Some(parent) = img_path.parent() {
            std::fs::create_dir_all(writing_directory.join(parent)).or(Err(format!(
                "failed to create parent dir for an image: {img_path}"
            )))?;
        }
        let target = writing_directory.join(img_path.as_str());
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(target)
            .or(Err(format!("failed to create file for image: {img_path}")))?
            .write_all(img)
            .or(Err(format!(
                "failed to write image bytes to file: {img_path}"
            )))?;
    }
    // save tbins
    for (tbin_path, tbin) in pack_texture.tbins.iter() {
        if let Some(parent) = tbin_path.parent() {
            std::fs::create_dir_all(writing_directory.join(parent)).or(Err(format!(
                "failed to create parent dir of tbin: {tbin_path}"
            )))?;
        }
        let mut bytes: Vec<u8> =
            Vec::with_capacity(8 + tbin.nodes.len() * std::mem::size_of::<Vec3>());
        bytes.extend_from_slice(&tbin.version.to_ne_bytes());
        bytes.extend_from_slice(&tbin.map_id.to_ne_bytes());
        for node in &tbin.nodes {
            let node = &node.0;
            bytes.extend_from_slice(&node[0].to_ne_bytes());
            bytes.extend_from_slice(&node[1].to_ne_bytes());
            bytes.extend_from_slice(&node[2].to_ne_bytes());
        }
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(writing_directory.join(tbin_path.as_str()))
            .or(Err(format!("failed to create tbin file: {tbin_path}")))?
            .write_all(bytes.as_slice())
            .or(Err(format!("failed to write tbin to file: {tbin_path}")))?;
    }
    Ok(())
}

fn recursive_cat_serializer(
    tree: &mut Xot,
    names: &XotAttributeNameIDs,
    cats: &OrderedHashMap<Uuid, Category>,
    parent: Node,
) -> Result<(), String> {
    for (_, cat) in cats {
        let cat_node = tree.new_element(names.marker_category);
        tree.append(parent, cat_node)
            .or(Err("Could not insert category node"))?;
        {
            let ele = tree.element_mut(cat_node).unwrap();
            ele.set_attribute(names.display_name, &cat.display_name);
            ele.set_attribute(names.guid, BASE64_ENGINE.encode(cat.guid));
            // let cat_name = tree.add_name(cat_name);
            ele.set_attribute(names.name, &cat.relative_category_name);
            // no point in serializing default values
            if !cat.default_enabled {
                ele.set_attribute(names.default_enabled, "0");
            }
            if cat.separator {
                ele.set_attribute(names.separator, "1");
            }
            cat.props.serialize_to_element(ele, names);
        }
        recursive_cat_serializer(tree, names, &cat.children, cat_node)?;
    }
    Ok(())
}
fn serialize_trail_to_element(trail: &Trail, ele: &mut Element, names: &XotAttributeNameIDs) {
    ele.set_attribute(names.guid, BASE64_ENGINE.encode(trail.guid));
    ele.set_attribute(names.category, &trail.category);
    ele.set_attribute(names.map_id, format!("{}", trail.map_id));
    ele.set_attribute(
        names._source_file_name,
        format!("{}", trail.source_file_uuid),
    );
    trail.props.serialize_to_element(ele, names);
}

fn serialize_marker_to_element(marker: &Marker, ele: &mut Element, names: &XotAttributeNameIDs) {
    let position = &marker.position.0;
    ele.set_attribute(names.xpos, format!("{}", position[0]));
    ele.set_attribute(names.ypos, format!("{}", position[1]));
    ele.set_attribute(names.zpos, format!("{}", position[2]));
    ele.set_attribute(names.guid, BASE64_ENGINE.encode(marker.guid));
    ele.set_attribute(names.map_id, format!("{}", marker.map_id));
    ele.set_attribute(names.category, &marker.category);
    ele.set_attribute(
        names._source_file_name,
        format!("{}", marker.source_file_uuid),
    );
    marker.attrs.serialize_to_element(ele, names);
}

fn serialize_route_to_element(
    tree: &mut Xot,
    route: &Route,
    parent: &Node,
    names: &XotAttributeNameIDs,
) -> Result<(), String> {
    let route_node = tree.new_element(names.route);
    tree.append(*parent, route_node)
        .or(Err("failed to append route to pois"))?;
    let ele = tree.element_mut(route_node).unwrap();

    let reset_position = &route.reset_position.0;
    ele.set_attribute(names.category, route.category.clone());
    ele.set_attribute(names.resetposx, format!("{}", reset_position[0]));
    ele.set_attribute(names.resetposy, format!("{}", reset_position[1]));
    ele.set_attribute(names.resetposz, format!("{}", reset_position[2]));
    ele.set_attribute(names.reset_range, format!("{}", route.reset_range));
    ele.set_attribute(names.name, route.name.clone());
    ele.set_attribute(names.guid, BASE64_ENGINE.encode(route.guid));
    ele.set_attribute(names.map_id, format!("{}", route.map_id));
    ele.set_attribute(names.texture, "default_trail_texture.png");
    ele.set_attribute(
        names._source_file_name,
        format!("{}", route.source_file_uuid),
    );
    for pos in &route.path {
        let pos = &pos.0;
        let child = tree.new_element(names.poi);
        tree.append(route_node, child)
            .or(Err("Could not inser child node"))?;
        let child_elt = tree.element_mut(child).unwrap();
        child_elt.set_attribute(names.xpos, format!("{}", pos.x));
        child_elt.set_attribute(names.ypos, format!("{}", pos.y));
        child_elt.set_attribute(names.zpos, format!("{}", pos.z));
        //child_elt.set_attribute(names.guid, BASE64_ENGINE.encode(uuid::Uuid::new_v4()));
    }
    Ok(())
}
