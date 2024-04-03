use joko_core::RelativePath;

use crate::{
    pack::{Category, RawCategory, CommonAttributes, Marker, PackCore, TBin, TBinStatus, Trail, MapData, Route, prefix_parent},
    BASE64_ENGINE,
};
use base64::Engine;
use cap_std::fs_utf8::Dir;
use glam::Vec3;
use indexmap::IndexMap;
use miette::{bail, Context, IntoDiagnostic, Result};
use std::{collections::{VecDeque, HashMap}, io::Read, sync::Arc};
use ordered_hash_map::OrderedHashMap;
use tracing::{debug, info, info_span, instrument, trace, warn};
use uuid::Uuid;
use xot::{Node, Xot, Element};

use super::XotAttributeNameIDs;

pub(crate) fn load_pack_core_from_dir(dir: &Dir) -> Result<PackCore> {
    //FIXME: this should return two elements:
    //called from already parsed data
    let mut pack = PackCore::default();
    pack.uuid = Uuid::new_v4();
    // walks the directory and loads all files into the hashmap
    recursive_walk_dir_and_read_images_and_tbins(
        dir,
        &mut pack.textures,
        &mut pack.tbins,
        &RelativePath::default(),
    )
    .wrap_err("failed to walk dir when loading a markerpack")?;

    //categories are required to register other objects
    let cats_xml = dir
        .read_to_string("categories.xml")
        .into_diagnostic()
        .wrap_err("failed to read categories.xml")?;
    let categories_file = String::from("categories.xml");
    parse_categories_file(&categories_file, &cats_xml, &mut pack)
        .wrap_err("failed to parse category file")?;

    // parse map data of the pack
    for entry in dir
        .entries()
        .into_diagnostic()
        .wrap_err("failed to read entries of pack dir")?
    {
        let entry = entry
            .into_diagnostic()
            .wrap_err("entry error whiel reading xml files")?;

        let name = entry
            .file_name()
            .into_diagnostic()
            .wrap_err("map data entry name not utf-8")?
            .to_string();

        if name.ends_with(".xml") {
            if let Some(name_as_str) = name.strip_suffix(".xml") {
                match name_as_str {
                    "categories" => {
                        //already done
                    }
                    map_id => {
                        // parse map file
                        let span_guard = info_span!("map", map_id).entered();
                        if let Ok(map_id) = map_id.parse::<u32>() {
                            let mut xml_str = String::new();
                            entry
                                .open()
                                .into_diagnostic()
                                .wrap_err("failed to open xml file")?
                                .read_to_string(&mut xml_str)
                                .into_diagnostic()
                                .wrap_err("faield to read xml string")?;
                            parse_map_file(map_id, &xml_str, &mut pack).wrap_err_with(|| {
                                miette::miette!("error parsing map file: {map_id}")
                            })?;
                        } else {
                            info!("unrecognized xml file {map_id}")
                        }
                        std::mem::drop(span_guard);
                    }
                }
            }
        } else {
            trace!("file ignored: {name}")
        }
    }
    info!("Entities registered (category + markers): {}", pack.entities_parents.len());
    info!("Categories registered: {}", pack.all_categories.len());
    info!("Markers registered: {}", pack.entities_parents.len() - pack.all_categories.len());
    info!("Maps registered: {}", pack.maps.len());
    info!("Textures registered: {}", pack.textures.len());
    info!("Trail binaries registered: {}", pack.tbins.len());
    Ok(pack)
}


fn recursive_walk_dir_and_read_images_and_tbins(
    dir: &Dir,
    images: &mut OrderedHashMap<RelativePath, Vec<u8>>,
    tbins: &mut OrderedHashMap<RelativePath, TBin>,
    parent_path: &RelativePath,
) -> Result<()> {
    for entry in dir
        .entries()
        .into_diagnostic()
        .wrap_err("failed to get directory entries")?
    {
        let entry = entry
            .into_diagnostic()
            .wrap_err("dir entry error when iterating dir entries")?;
        let name = entry.file_name().into_diagnostic()?;
        let path = parent_path.join_str(&name);

        if entry
            .file_type()
            .into_diagnostic()
            .wrap_err("failed to get file type")?
            .is_file()
        {
            if path.ends_with(".png") || path.ends_with(".trl") {
                let mut bytes = vec![];
                entry
                    .open()
                    .into_diagnostic()
                    .wrap_err("failed to open file")?
                    .read_to_end(&mut bytes)
                    .into_diagnostic()
                    .wrap_err("failed to read file contents")?;
                if name.ends_with(".png") {
                    images.insert(path.clone(), bytes);
                } else if name.ends_with(".trl") {
                    if let Some(tbs) = parse_tbin_from_slice(&bytes) {
                        let is_closed: bool = tbs.closed;
                        if is_closed {
                            if tbs.iso_x {}
                            if tbs.iso_y {}
                            if tbs.iso_z {}
                        }
                        tbins.insert(path, tbs.tbin);
                    } else {
                        info!("invalid tbin: {path}");
                    }
                }
            }
        } else {
            recursive_walk_dir_and_read_images_and_tbins(
                &entry.open_dir().into_diagnostic()?,
                images,
                tbins,
                &path,
            )?;
        }
    }
    Ok(())
}
fn parse_tbin_from_slice(bytes: &[u8]) -> Option<TBinStatus> {
    let content_length = bytes.len();
    // content_length must be atleast 8 to contain version + map_id
    if content_length < 8 {
        info!("failed to parse tbin because the len is less than 8");
        return None;
    }

    let mut version_bytes = [0_u8; 4];
    version_bytes.copy_from_slice(&bytes[4..8]);
    let version = u32::from_ne_bytes(version_bytes);
    let mut map_id_bytes = [0_u8; 4];
    map_id_bytes.copy_from_slice(&bytes[4..8]);
    let map_id = u32::from_ne_bytes(map_id_bytes);

    let zero = Vec3{x:0.0, y:0.0, z:0.0};

    // this will either be empty vec or series of vec3s.
    let nodes: VecDeque<Vec3> = bytes[8..]
        .chunks_exact(12)
        .map(|float_bytes| {
            // make [f32 ;3] out of those 12 bytes
            let arr = [
                f32::from_le_bytes([
                    // first float
                    float_bytes[0],
                    float_bytes[1],
                    float_bytes[2],
                    float_bytes[3],
                ]),
                f32::from_le_bytes([
                    // second float
                    float_bytes[4],
                    float_bytes[5],
                    float_bytes[6],
                    float_bytes[7],
                ]),
                f32::from_le_bytes([
                    // third float
                    float_bytes[8],
                    float_bytes[9],
                    float_bytes[10],
                    float_bytes[11],
                ]),
            ];

            Vec3::from_array(arr)
        })
        .collect();

    //There are zeroes in trails. Reason may be either bad trail or used as a separator for several trails in same file.
    let mut iso_x = false;
    let mut iso_y = false;
    let mut iso_z = false;
    let mut closed = false;
    let mut resulting_nodes : Vec<Vec3> = Vec::new();
    if nodes.len() > 0 {
        let ref_node = nodes[0];
        let mut c_iso_x = true;
        let mut c_iso_y = true;
        let mut c_iso_z = true;
        // ensure there is not too much distance between two points, if it is the case, we do split the path in several parts
        resulting_nodes.push(ref_node);
        for (a, b) in nodes.iter().zip(nodes.iter().skip(1)) {
            //ignore zeroes since they would be separators
            if a.distance_squared(zero) > 0.01 && b.distance_squared(zero) > 0.01 {
                let distance_to_next_point = a.distance_squared(*b);
                let mut current_cursor = distance_to_next_point;
                while current_cursor > 400.0 {
                    let c = a.lerp(*b, 1.0 - current_cursor / distance_to_next_point);
                    resulting_nodes.push(c);
                    current_cursor -= 400.0;
                }
            }
            resulting_nodes.push(*b);
        }
        for node in &nodes {
            if resulting_nodes.len() > 1 {
                //TODO: load epsilon from a configuration somewhere, with a default value
                if (node.x - ref_node.x).abs() < 0.1 {
                    c_iso_x = false;
                }
                if (node.y - ref_node.y).abs() < 0.1 {
                    c_iso_y = false;
                }
                if (node.z - ref_node.z).abs() < 0.1 {
                    c_iso_z = false;
                }
            }
        }
        iso_x = c_iso_x;
        iso_y = c_iso_y;
        iso_z = c_iso_z;
        if nodes.len() > 1 {// TODO: get this threshold from configuration
            closed = nodes.front().unwrap().distance(*nodes.back().unwrap()).abs() < 0.1
        }
    }
    Some(TBinStatus{
        tbin: TBin {
            map_id,
            version,
            nodes: resulting_nodes,
        },
        iso_x,
        iso_y,
        iso_z,
        closed
    })
}

fn parse_categories(
    tree: &Xot,
    tags: impl Iterator<Item = Node>,
    first_pass_categories: &mut OrderedHashMap<String, RawCategory>,
    names: &XotAttributeNameIDs,
) {
    //called once per file
    parse_categories_recursive(tree, tags, first_pass_categories, names, None);
    
}


// a recursive function to parse the marker category tree.
fn parse_categories_recursive(
    tree: &Xot,
    tags: impl Iterator<Item = Node>,
    first_pass_categories: &mut OrderedHashMap<String, RawCategory>,
    names: &XotAttributeNameIDs,
    parent_name: Option<String>,
) {
    for tag in tags {
        let ele = match tree.element(tag) {
            Some(ele) => ele,
            None => continue,
        };
        if ele.name() != names.marker_category {
            continue;
        }

        let name = ele
            .get_attribute(names.name)
            .or(ele.get_attribute(names.CapitalName))
            .unwrap_or_default()
            .to_lowercase();
        if name.is_empty() {
            continue;
        }
        let mut ca = CommonAttributes::default();
        ca.update_common_attributes_from_element(ele, names);

        /*
        FIXME: how to handle both
            orphans
            out of order evaluation => mark the current marker category to be skipped and not inserted, this is an orphan for later reinsertion
        if the category has a Display name, then the name is relative, if not, it means this is defined somewhere else and name is absolute.
            => have a "late insertion" container
        */
        let display_name = ele.get_attribute(names.display_name).unwrap_or(&name);

        let separator = ele
            .get_attribute(names.separator)
            .unwrap_or_default()
            .parse()
            .map(|u: u8| u != 0)
            .unwrap_or_default();

        let default_enabled = ele
            .get_attribute(names.default_enabled)
            .unwrap_or_default()
            .parse()
            .map(|u: u8| u != 0)
            .unwrap_or(true);
        let guid = parse_guid(names, ele);
        let full_category_name: String = if let Some(parent_name) = &parent_name {
            format!("{}.{}", parent_name, name)
        } else {
            name.to_string()
        };
        trace!("recursive_marker_category_parser {} {} {:?}", name, guid, parent_name);
        if !first_pass_categories.contains_key(&full_category_name) {
            first_pass_categories.insert(full_category_name.clone(), RawCategory {
                guid,
                parent_name: parent_name.clone(),
                display_name: display_name.to_string(),
                relative_category_name: name.to_string(),
                full_category_name: full_category_name.clone(),
                separator,
                default_enabled,
                props: ca,
            });
        }
        parse_categories_recursive(
            tree,
            tree.children(tag),
            first_pass_categories,
            names,
            Some(full_category_name),
        );
    }
}

fn parse_categories_file(file_name: &String, cats_xml_str: &str, pack: &mut PackCore) -> Result<()> {
    let mut tree = xot::Xot::new();
    let xot_names = XotAttributeNameIDs::register_with_xot(&mut tree);
    let root_node = tree
        .parse(cats_xml_str)
        .into_diagnostic()
        .wrap_err("invalid xml")?;

    let overlay_data_node = tree
        .document_element(root_node)
        .into_diagnostic()
        .wrap_err("no doc element")?;

    if let Some(od) = tree.element(overlay_data_node) {
        let mut categories: IndexMap<Uuid, Category> = Default::default();
        if od.name() == xot_names.overlay_data {
            parse_category_categories_xml_recursive(
                &file_name,
                &tree,
                tree.children(overlay_data_node),
                pack,
                &mut categories,
                &xot_names,
                None,
                None,
            );
            trace!("loaded categories: {:?}", categories);
            pack.categories = categories;
            pack.register_categories();
        } else {
            bail!("root tag is not OverlayData")
        }
    } else {
        bail!("doc element is not element???");
    }
    Ok(())
}


fn parse_map_file(map_id: u32, map_xml_str: &str, pack: &mut PackCore) -> Result<()> {
    let mut tree = Xot::new();
    let root_node = tree
        .parse(map_xml_str)
        .into_diagnostic()
        .wrap_err("invalid xml")?;
    let names = XotAttributeNameIDs::register_with_xot(&mut tree);
    let overlay_data_node = tree
        .document_element(root_node)
        .into_diagnostic()
        .wrap_err("missing doc element")?;

    let overlay_data_element = tree
        .element(overlay_data_node)
        .ok_or_else(|| miette::miette!("no doc ele"))?;

    if overlay_data_element.name() != names.overlay_data {
        bail!("root tag is not OverlayData");
    }
    let pois = tree
        .children(overlay_data_node)
        .find(|node| match tree.element(*node) {
            Some(ele) => ele.name() == names.pois,
            None => false,
        })
        .ok_or_else(|| miette::miette!("missing pois node"))?;

    for poi_node in tree.children(pois) {
        if let Some(child) = tree.element(poi_node) {
            let full_category_name = child
                .get_attribute(names.category)
                .unwrap_or_default()
                .to_lowercase();
            if full_category_name.is_empty() {
                panic!("full_category_name is empty {:?} {:?}", map_xml_str, child);
            }
            let span_guard = info_span!("category", full_category_name).entered();

            let category_uuid = pack.get_or_create_category_uuid(&full_category_name);

            let raw_uid = child.get_attribute(names.guid);
            if raw_uid.is_none() {
                info!("This POI is either invalid or inside a Route {:?}", child);
                span_guard.exit();
                continue;
            }
            let guid = raw_uid.and_then(|guid| {
                    let mut buffer = [0u8; 20];
                    BASE64_ENGINE
                        .decode_slice(guid, &mut buffer)
                        .ok()
                        .and_then(|_| Uuid::from_slice(&buffer[..16]).ok())
                })
                .ok_or_else(|| miette::miette!("invalid guid {:?}", raw_uid))?;
            
            let source_file_name = child.get_attribute(names._source_file_name).unwrap_or_default().to_string();
            pack.source_files.insert(source_file_name.clone(), true);
            //TODO: route, difference with trail: trail is binary format while route is text => convert route into a trail
            if child.name() == names.route {
                debug!("Found a route in core pack {:?}", child);
                import_route_as_trail(pack, &names, &tree, &poi_node, child, full_category_name, &category_uuid, source_file_name)
            }
            else if child.name() == names.poi {
                debug!("Found a POI in core pack {:?}", child);
                if child
                    .get_attribute(names.map_id)
                    .and_then(|map_id| map_id.parse::<u32>().ok())
                    .ok_or_else(|| miette::miette!("invalid mapid"))?
                    != map_id
                {
                    bail!("mapid doesn't match the file name");
                }
                let xpos = child
                    .get_attribute(names.xpos)
                    .unwrap_or_default()
                    .parse::<f32>()
                    .into_diagnostic()?;
                let ypos = child
                    .get_attribute(names.ypos)
                    .unwrap_or_default()
                    .parse::<f32>()
                    .into_diagnostic()?;
                let zpos = child
                    .get_attribute(names.zpos)
                    .unwrap_or_default()
                    .parse::<f32>()
                    .into_diagnostic()?;
                let mut ca = CommonAttributes::default();
                ca.update_common_attributes_from_element(child, &names);

                pack.register_uuid(&full_category_name, &guid);
                let marker = Marker {
                    position: [xpos, ypos, zpos].into(),
                    map_id,
                    category: full_category_name,
                    parent: category_uuid.clone(),
                    attrs: ca,
                    guid,
                    source_file_name
                };

                if !pack.maps.contains_key(&map_id) {
                    pack.maps.insert(map_id, MapData::default());
                }
                pack.maps.get_mut(&map_id).unwrap().markers.insert(marker.guid, marker);
            } else if child.name() == names.trail {
                debug!("Found a trail in core pack {:?}", child);
                if child
                    .get_attribute(names.map_id)
                    .and_then(|map_id| map_id.parse::<u32>().ok())
                    .ok_or_else(|| miette::miette!("invalid mapid"))?
                    != map_id
                {
                    bail!("mapid doesn't match the file name");
                }
                let mut ca = CommonAttributes::default();
                ca.update_common_attributes_from_element(child, &names);

                pack.register_uuid(&full_category_name, &guid);
                let trail = Trail {
                    category: full_category_name,
                    parent: category_uuid.clone(),
                    map_id,
                    props: ca,
                    guid,
                    dynamic: false,
                    source_file_name
                };
                
                if !pack.maps.contains_key(&map_id) {
                    pack.maps.insert(map_id, MapData::default());
                }
                pack.maps.get_mut(&map_id).unwrap().trails.insert(trail.guid, trail);
            }
            span_guard.exit();
        }
    }
    Ok(())
}

// a temporary recursive function to parse the marker category tree.
fn parse_category_categories_xml_recursive(
    file_name: &String,
    tree: &Xot,
    tags: impl Iterator<Item = Node>,
    pack: &mut PackCore,
    cats: &mut IndexMap<Uuid, Category>,
    names: &XotAttributeNameIDs,
    parent_uuid: Option<Uuid>,
    parent_name: Option<String>,
) {
    for tag in tags {
        if let Some(ele) = tree.element(tag) {
            if ele.name() != names.marker_category {
                continue;
            }

            //TODO: if no display name, only keep the parent/enfant relationship
            let relative_category_name = ele.get_attribute(names.name)
                .or(ele.get_attribute(names.display_name)
                    .or(ele.get_attribute(names.CapitalName)
                )
            ).unwrap_or_default().to_lowercase();
            if relative_category_name.is_empty() {
                info!("category doesn't have a name attribute: {ele:#?}");
                continue;
            }
            let span_guard = info_span!("category", relative_category_name).entered();
            let mut ca = CommonAttributes::default();
            ca.update_common_attributes_from_element(ele, names);

            let display_name = ele.get_attribute(names.display_name).unwrap_or_default();

            let separator = match ele.get_attribute(names.separator).unwrap_or("0") {
                "0" => false,
                "1" => true,
                ors => {
                    info!("separator attribute has invalid value: {ors}");
                    false
                }
            };

            let default_enabled = match ele.get_attribute(names.default_enabled).unwrap_or("1") {
                "0" => false,
                "1" => true,
                ors => {
                    info!("default_enabled attribute has invalid value: {ors}");
                    true
                }
            };
            let full_category_name: String = if let Some(parent_name) = &parent_name {
                format!("{}.{}", parent_name, relative_category_name)
            } else {
                relative_category_name.to_string()
            };
            let guid = parse_guid(names, ele);
            trace!("recursive_marker_category_parser_categories_xml {} {} {:?}", full_category_name, guid, parent_uuid);
            if display_name.is_empty() {
                assert!(parent_name.is_none());
                parse_category_categories_xml_recursive(
                    file_name,
                    tree,
                    tree.children(tag),
                    pack,
                    cats,
                    names,
                    Some(guid),
                    Some(full_category_name),
                );
            } else {
                let current_category = cats
                    .entry(guid)
                    .or_insert_with(|| Category {
                        guid,
                        parent: parent_uuid.clone(),
                        display_name: display_name.to_string(),
                        relative_category_name: relative_category_name.to_string(),
                        full_category_name: full_category_name.clone(),
                        separator,
                        default_enabled,
                        props: ca,
                        children: Default::default(),
                    });
                parse_category_categories_xml_recursive(
                        file_name,
                        tree,
                        tree.children(tag),
                        pack,
                        &mut current_category.children,
                        names,
                        Some(guid),
                        Some(full_category_name),
                    );
            };
            
            std::mem::drop(span_guard);
        } else {
            //it may be a comment, a space, anything
            //info!("In file {}, ignore node {:?}", file_name, tag);
        }
    }
}

/// This first parses all the files in a zipfile into the memory and then it will try to parse a zpack out of all the files.
/// will return error if there's an issue with zipfile.
///
/// but any other errors like invalid attributes or missing markers etc.. will just be logged.
/// the intention is "best effort" parsing and not "validating" xml marker packs.
/// we will ignore any issues like unknown attributes or xml tags. "unknown" attributes means Any attributes that jokolay doesn't parse into Zpack.
#[instrument(skip_all)]
pub(crate) fn get_pack_from_taco_zip(taco: &[u8]) -> Result<PackCore> {
    //FIXME: there might be a problem where the elements are not displayed immediately after save
    //called to import a new pack
    // all the contents of ZPack
    let mut pack = PackCore::default();
    // parse zip file
    let mut zip_archive = zip::ZipArchive::new(std::io::Cursor::new(taco))
        .into_diagnostic()
        .wrap_err("failed to read zip archive")?;

    // file paths of different file types
    let mut images = vec![];
    let mut tbins = vec![];
    let mut xmls = vec![];
    // we collect the names first, because reading a file from zip is a mutating operation.
    // So, we can't iterate AND read the file at the same time
    for name in zip_archive.file_names() {
        let name_as_string = name.to_string();
        if name_as_string.ends_with(".png") {
            images.push(name_as_string);
        } else if name_as_string.ends_with(".trl") {
            tbins.push(name_as_string);
        } else if name_as_string.ends_with(".xml") {
            xmls.push(name_as_string);
        } else if name_as_string.replace("\\", "/").ends_with('/') {
            // directory. so, we can silently ignore this.
        } else {
            info!("ignoring file: {name}");
        }
    }
    xmls.sort();//build back the intended order in folder, since zip_archive may not give the files in order.
    for name in images {
        let span = info_span!("load image", name).entered();
        let file_path: RelativePath = name.replace("\\", "/").parse().unwrap();
        if let Some(bytes) = read_file_bytes_from_zip_by_name(&name, &mut zip_archive) {
            match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) {
                Ok(_) => assert!(
                    pack.textures.insert(file_path.clone(), bytes).is_none(),
                    "duplicate image file {name}"
                ),
                Err(e) => {
                    info!(?e, "failed to parse image file");
                }
            }
        }
        std::mem::drop(span);
    }

    for name in tbins {
        let span = info_span!("load tbin {name}").entered();

        let file_path: RelativePath = name.replace("\\", "/").parse().unwrap();
        if let Some(bytes) = read_file_bytes_from_zip_by_name(&name, &mut zip_archive) {
            if let Some(tbs) = parse_tbin_from_slice(&bytes) {
                let is_closed: bool = tbs.closed;
                if is_closed {
                    if tbs.iso_x {}
                    if tbs.iso_y {}
                    if tbs.iso_z {}
                }
                assert!(
                    pack.tbins.insert(file_path, tbs.tbin).is_none(),
                    "duplicate tbin file {name}"
                );
            } else {
                info!("failed to parse tbin from slice: {file_path}");
            }
        } else {
            info!(name, "failed to read tbin from zipfile");
        }
        std::mem::drop(span);
    }

    let span_guard_categories = info_span!("deserialize xml: categories").entered();

    //first pass: categories only
    let span_guard_first_pass = info_span!("deserialize xml first pass: load MarkerCategory").entered();
    let mut first_pass_categories: OrderedHashMap<String, RawCategory> = Default::default();
    for source_file_name in xmls.iter() {
        let mut xml_str = String::new();
        let span_guard = info_span!("deserialize xml first pass: load file", source_file_name).entered();
        if zip_archive
            .by_name(&source_file_name)
            .ok()
            .and_then(|mut file| file.read_to_string(&mut xml_str).ok())
            .is_none()
        {
            info!("failed to read file from zip");
            continue;
        };

        let filtered_xml_str = crate::rapid_filter_rust(xml_str);
        let mut tree = Xot::new();
        let root_node = match tree.parse(&filtered_xml_str) {
            Ok(root) => root,
            Err(e) => {
                info!(?e, "failed to parse as xml");
                continue;
            }
        };
        let names = XotAttributeNameIDs::register_with_xot(&mut tree);
        let od = match tree
            .document_element(root_node)
            .ok()
            .filter(|od| (tree.element(*od).unwrap().name() == names.overlay_data))
        {
            Some(od) => od,
            None => {
                info!("missing overlay data tag");
                continue;
            }
        };

        parse_categories(&tree, tree.children(od), &mut first_pass_categories, &names);
        drop(span_guard);
    }
    span_guard_first_pass.exit();

    //second pass: orphan categories
    let span_guard_second_pass = info_span!("deserialize xml second pass: orphan categories").entered();
    for source_file_name in xmls.iter() {
        let mut xml_str = String::new();
        let span_guard = info_span!("deserialize xml second pass: load file", source_file_name).entered();
        if zip_archive
            .by_name(&source_file_name)
            .ok()
            .and_then(|mut file| file.read_to_string(&mut xml_str).ok())
            .is_none()
        {
            info!("failed to read file from zip");
            continue;
        };

        let filtered_xml_str = crate::rapid_filter_rust(xml_str);
        let mut tree = Xot::new();
        let root_node = match tree.parse(&filtered_xml_str) {
            Ok(root) => root,
            Err(e) => {
                info!(?e, "failed to parse as xml");
                continue;
            }
        };
        let names = XotAttributeNameIDs::register_with_xot(&mut tree);
        let od = match tree
            .document_element(root_node)
            .ok()
            .filter(|od| (tree.element(*od).unwrap().name() == names.overlay_data))
        {
            Some(od) => od,
            None => {
                info!("missing overlay data tag");
                continue;
            }
        };
        let pois = match tree.children(od).find(|node| {
            tree.element(*node)
                .map(|ele: &xot::Element| ele.name() == names.pois)
                .unwrap_or_default()
        }) {
            Some(pois) => pois,
            None => {
                info!("missing pois tag");
                continue;
            }
        };

        for child_node in tree.children(pois) {
            let child = match tree.element(child_node) {
                Some(ele) => ele,
                None => continue,
            };
            let full_category_name = child
                .get_attribute(names.category)
                .unwrap_or_default()
                .to_lowercase();
            if full_category_name.is_empty() {
                //ignore it silently since it might be a Route
                //info!("full_category_name is empty {:?}", child);
                continue;
            }
            if !pack.category_exists(&full_category_name) && ! first_pass_categories.contains_key(&full_category_name) {
                let category_uuid = Uuid::new_v4();
                first_pass_categories.insert(full_category_name.clone(), RawCategory{
                    default_enabled: true,
                    guid: category_uuid,
                    parent_name: prefix_parent(&full_category_name, '.'),
                    display_name: full_category_name.clone(),
                    full_category_name: full_category_name.clone(),
                    relative_category_name: full_category_name.clone(),
                    props: Default::default(),
                    separator: false
                });
                info!("There is an orphan missing category '{}' which was created", full_category_name);
            }
        }
        drop(span_guard);
    }
    span_guard_second_pass.exit();
    
    pack.categories = Category::reassemble(&first_pass_categories, &mut pack.late_discovery_categories);
    pack.register_categories();

    //third and last pass: elements
    let span_guard_third_pass = info_span!("deserialize xml third pass: load elements").entered();
    for source_file_name in xmls.iter() {
        let mut xml_str = String::new();
        let span_guard = info_span!("deserialize xml third pass load file ", source_file_name).entered();
        if zip_archive
            .by_name(&source_file_name)
            .ok()
            .and_then(|mut file| file.read_to_string(&mut xml_str).ok())
            .is_none()
        {
            info!("failed to read file from zip");
            continue;
        };

        let filtered_xml_str = crate::rapid_filter_rust(xml_str);
        let mut tree = Xot::new();
        let root_node = match tree.parse(&filtered_xml_str) {
            Ok(root) => root,
            Err(e) => {
                info!(?e, "failed to parse as xml");
                continue;
            }
        };
        let names = XotAttributeNameIDs::register_with_xot(&mut tree);
        let od = match tree
            .document_element(root_node)
            .ok()
            .filter(|od| (tree.element(*od).unwrap().name() == names.overlay_data))
        {
            Some(od) => od,
            None => {
                info!("missing overlay data tag");
                continue;
            }
        };

        let pois = match tree.children(od).find(|node| {
            tree.element(*node)
                .map(|ele: &xot::Element| ele.name() == names.pois)
                .unwrap_or_default()
        }) {
            Some(pois) => pois,
            None => {
                info!("missing pois tag");
                continue;
            }
        };

        for child_node in tree.children(pois) {
            let child = match tree.element(child_node) {
                Some(ele) => ele,
                None => continue,
            };
            let full_category_name = child
                .get_attribute(names.category)
                .unwrap_or_default()
                .to_lowercase();
            if full_category_name.is_empty() {
                info!("full_category_name is empty {:?}", child);
                continue;
            }
            if ! pack.category_exists(&full_category_name) {
                panic!("Missing category {}, previous pass should have taken care of this", full_category_name);
            }
            let category_uuid = pack.get_or_create_category_uuid(&full_category_name);

            debug!("import element: {:?}", child);
            if child.name() == names.poi {
                import_poi(&mut pack, &names, &child, full_category_name, &category_uuid, source_file_name.clone());
            } else if child.name() == names.trail {
                import_trail(&mut pack, &names, &child, full_category_name, &category_uuid, source_file_name.clone());
            } else if child.name() == names.route {
                import_route_as_trail(&mut pack, &names, &tree, &child_node, &child, full_category_name, &category_uuid, source_file_name.clone());
            } else {
                info!("unknown element: {:?}", child);
            }
        }

        drop(span_guard);
    }
    span_guard_third_pass.exit();
    span_guard_categories.exit();
    Ok(pack)
}

fn parse_guid(names: &XotAttributeNameIDs, child: &Element) -> Uuid{
    child
    .get_attribute(names.guid)
    .and_then(|guid| {
        let mut buffer = [0u8; 20];
        BASE64_ENGINE
            .decode_slice(guid, &mut buffer)
            .ok()
            .and_then(|_| Uuid::from_slice(&buffer[..16]).ok())
            .or_else(|| {
                info!(guid, "failed to deserialize guid");
                None
            })
    })
    .unwrap_or_else(Uuid::new_v4)
}

fn parse_marker(pack: &mut PackCore, names: &XotAttributeNameIDs, poi_element: &Element, category_name: &String, category_uuid: &Uuid, source_file_name: String) -> Option<Marker> {
    if let Some(map_id) = poi_element
        .get_attribute(names.map_id)
        .and_then(|map_id| map_id.parse::<u32>().ok())
    {
        let xpos = poi_element
            .get_attribute(names.xpos)
            .unwrap_or_default()
            .parse::<f32>()
            .unwrap_or_default();
        let ypos = poi_element
            .get_attribute(names.ypos)
            .unwrap_or_default()
            .parse::<f32>()
            .unwrap_or_default();
        let zpos = poi_element
            .get_attribute(names.zpos)
            .unwrap_or_default()
            .parse::<f32>()
            .unwrap_or_default();
        let mut common_attributes = CommonAttributes::default();
        common_attributes.update_common_attributes_from_element(poi_element, &names);
        if let Some(icon_file) = common_attributes.get_icon_file() {
            if !pack.textures.contains_key(icon_file) {
                info!(%icon_file, "failed to find this texture in this pack");
            }
        } else if let Some(icf) = poi_element.get_attribute(names.icon_file) {
            info!(icf, "marker's icon file attribute failed to parse");
        }
        Some(Marker {
            position: [xpos, ypos, zpos].into(),
            map_id,
            category: category_name.clone(),
            parent: category_uuid.clone(),
            attrs: common_attributes,
            guid: parse_guid(names, poi_element),
            source_file_name
        })
    } else {
        info!("missing map id");
        None
    }
}

fn parse_position(names: &XotAttributeNameIDs, poi_element: &Element) -> Vec3 {
    let x = poi_element
        .get_attribute(names.xpos)
        .unwrap_or_default()
        .parse::<f32>()
        .unwrap_or_default();
    let y = poi_element
        .get_attribute(names.ypos)
        .unwrap_or_default()
        .parse::<f32>()
        .unwrap_or_default();
    let z = poi_element
        .get_attribute(names.zpos)
        .unwrap_or_default()
        .parse::<f32>()
        .unwrap_or_default();
    Vec3{x, y, z}
}

fn parse_route(
    _pack: &mut PackCore, 
    names: &XotAttributeNameIDs,
    tree: &Xot, 
    route_node: &Node, 
    route_element: &Element, 
    category_name: &String, 
    category_uuid: &Uuid, 
    source_file_name: String
) -> Option<Route> {
    
    let mut path: Vec<Vec3> = Vec::new();
    let resetposx = route_element
        .get_attribute(names.resetposx)
        .unwrap_or_default()
        .parse::<f32>()
        .unwrap_or_default();
    let resetposy = route_element
        .get_attribute(names.resetposy)
        .unwrap_or_default()
        .parse::<f32>()
        .unwrap_or_default();
    let resetposz = route_element
        .get_attribute(names.resetposz)
        .unwrap_or_default()
        .parse::<f32>()
        .unwrap_or_default();
    let reset_position = Vec3::new(resetposx, resetposy, resetposz);
    let reset_range = route_element.get_attribute(names.reset_range).and_then(|map_id| map_id.parse::<f64>().ok());
    let name = route_element.get_attribute(names.name).or(route_element.get_attribute(names.CapitalName));

    if name.is_none() {
        info!("route element is missing name: {route_element:?}");
        return None;
    }
    let mut category: String = category_name.clone();
    let mut map_id: Option<u32> = route_element.get_attribute(names.map_id)
        .and_then(|map_id| map_id.parse::<u32>().ok());
    for child_node in tree.children(*route_node) {
        let child = match tree.element(child_node) {
            Some(ele) => ele,
            None => continue,
        };
        if child.name() == names.poi {
            let marker = parse_position(&names, child);
            path.push(marker);
            if let Some(cat) = child.get_attribute(names.category) {
                if category.is_empty() {
                    category = cat.to_string();
                }
            }
            if map_id.is_none() {
                if let Some(node_map_id) = child
                    .get_attribute(names.map_id)
                    .and_then(|map_id| map_id.parse::<u32>().ok())
                {
                    map_id = Some(node_map_id);
                }
            }
        }
    }
    if category.is_empty() {
        info!("Could not find a category for route element: {route_element:?}");
        return None;
    }
    if map_id.is_none() {
        info!("Could not find a map_id for route element: {route_element:?}");
        return None;
    }
    debug!("found route with {:?} elements {route_element:?}", path.len());

    Some(Route {
        category,
        parent: category_uuid.clone(), 
        path,
        reset_position,
        reset_range: reset_range.unwrap_or(0.0),
        map_id: map_id.unwrap(),
        name: name.unwrap().into(),
        guid: parse_guid(names, &route_element),
        source_file_name,
    })
}


fn parse_trail(pack: &mut PackCore, names: &XotAttributeNameIDs, trail_element: &Element, category_name: &String, category_uuid: &Uuid, source_file_name: String) -> Option<Trail> {
    //http://www.gw2taco.com/2022/04/a-proper-marker-editor-finally.html
    if let Some(map_id) = trail_element
     .get_attribute(names.trail_data)
        .and_then(|trail_data| {
            let path: RelativePath = trail_data.parse().unwrap();
            pack.tbins.get(&path).map(|tb| tb.map_id)
        })
    {
        let mut common_attributes = CommonAttributes::default();
        common_attributes.update_common_attributes_from_element(trail_element, &names);

        if let Some(tex) = common_attributes.get_texture() {
            if !pack.textures.contains_key(tex) {
                info!(%tex, "failed to find this texture in this pack");
            }
        }

        Some(Trail {
            category: category_name.clone(),
            parent: category_uuid.clone(),
            map_id,
            props: common_attributes,
            guid: parse_guid(names, trail_element),
            dynamic: false,
            source_file_name,
        })
    } else {
        let td = trail_element.get_attribute(names.trail_data);
        let rp: RelativePath = td.unwrap_or_default().parse().unwrap();
        let tbin = pack.tbins.get(&rp).map(|tbin| (tbin.map_id, tbin.version));
        info!("missing map_id: {td:?} {rp} {tbin:?}");
        None
    }

}

fn import_poi(pack: &mut PackCore, names: &XotAttributeNameIDs, poi_element: &Element, category_name: String, category_uuid: &Uuid, source_file_name: String) {
    if let Some(marker) = parse_marker(pack, names, poi_element, &category_name, category_uuid, source_file_name) {
        pack.register_uuid(&category_name, &marker.guid);
        if !pack.maps.contains_key(&marker.map_id) {
            pack.maps.insert(marker.map_id, MapData::default());
        }
        pack.maps.get_mut(&marker.map_id).unwrap().markers.insert(marker.guid, marker);
    } else {
        debug!("Could not parse POI");
    }
}


fn import_trail(pack: &mut PackCore, names: &XotAttributeNameIDs, trail_element: &Element, category_name: String, category_uuid: &Uuid, source_file_name: String) {
    if let Some(trail) = parse_trail(pack, names, trail_element, &category_name, category_uuid, source_file_name) {
        pack.register_uuid(&category_name, &trail.guid);
        if !pack.maps.contains_key(&trail.map_id) {
            pack.maps.insert(trail.map_id, MapData::default());
        }
        pack.maps.get_mut(&trail.map_id).unwrap().trails.insert(trail.guid, trail);
    } else {
        debug!("Could not parse Trail");
    }

}

fn route_to_tbin(route: &Route) -> TBin {
    assert!( route.path.len() > 1);
    TBin {
        map_id: route.map_id,
        version: 0,
        nodes: route.path.clone(),
    }
}

fn route_to_trail(route: &Route, file_path: &RelativePath) -> Trail {
    let mut props = CommonAttributes::default();
    props.set_texture(None);
    props.set_trail_data(Some(file_path.clone()));
    debug!("Build dynamic trail {}", route.guid);
    Trail {
        map_id: route.map_id,
        category: route.category.clone(),
        parent: route.parent.clone(),
        guid: route.guid,
        props: props,
        dynamic: true,
        source_file_name: route.source_file_name.clone(),
    }
}

fn import_route_as_trail(
    pack: &mut PackCore, 
    names: &XotAttributeNameIDs, 
    tree: &Xot, 
    route_node: &Node, 
    route_element: &Element, 
    category_name: String, 
    category_uuid: &Uuid,
    source_file_name: String
) {
    if let Some(route) = parse_route(pack, names, tree, route_node, route_element, &category_name, category_uuid, source_file_name) {
        let file_name = format!("data/dynamic_trails/{}.trl", &route.guid);
        let file_path: RelativePath = file_name.parse().unwrap();
        let trail = route_to_trail(&route, &file_path);
        let tbin = route_to_tbin(&route);
        pack.register_uuid(&category_name, &route.guid);
        pack.tbins.insert(file_path, tbin);//there may be duplicates since we load and save each time
        if !pack.maps.contains_key(&trail.map_id) {
            pack.maps.insert(trail.map_id, MapData::default());
        }
        pack.maps.get_mut(&trail.map_id).unwrap().trails.insert(trail.guid, trail);
        pack.maps.get_mut(&route.map_id).unwrap().routes.insert(route.guid, route);
    } else {
        info!("Could not parse route {:?}", route_element);
    }
}

#[instrument(skip(zip_archive))]
fn read_file_bytes_from_zip_by_name<T: std::io::Read + std::io::Seek>(
    name: &str,
    zip_archive: &mut zip::ZipArchive<T>,
) -> Option<Vec<u8>> {
    let mut bytes = vec![];
    match zip_archive.by_name(name) {
        Ok(mut file) => match file.read_to_end(&mut bytes) {
            Ok(size) => {
                if size == 0 {
                    info!("empty file {name}");
                } else {
                    return Some(bytes);
                }
            }
            Err(e) => {
                info!(?e, "failed to read file");
            }
        },
        Err(e) => {
            info!(?e, "failed to get file from zip");
        }
    }
    None
}


// #[cfg(test)]
// mod test {

//     use indexmap::IndexMap;
//     use rstest::*;

//     use semver::Version;
//     use similar_asserts::assert_eq;
//     use std::io::Write;
//     use std::sync::Arc;

//     use zip::write::FileOptions;
//     use zip::ZipWriter;

//     use crate::{
//         pack::{xml::zpack_from_xml_entries, Pack, MARKER_PNG},
//         INCHES_PER_METER,
//     };

//     const TEST_XML: &str = include_str!("test.xml");
//     const TEST_MARKER_PNG_NAME: &str = "marker.png";
//     const TEST_TRL_NAME: &str = "basic.trl";

//     #[fixture]
//     #[once]
//     fn test_zip() -> Vec<u8> {
//         let mut writer = ZipWriter::new(std::io::Cursor::new(vec![]));
//         // category.xml
//         writer
//             .start_file("category.xml", FileOptions::default())
//             .expect("failed to create category.xml");
//         writer
//             .write_all(TEST_XML.as_bytes())
//             .expect("failed to write category.xml");
//         // marker.png
//         writer
//             .start_file(TEST_MARKER_PNG_NAME, FileOptions::default())
//             .expect("failed to create marker.png");
//         writer
//             .write_all(MARKER_PNG)
//             .expect("failed to write marker.png");
//         // basic.trl
//         writer
//             .start_file(TEST_TRL_NAME, FileOptions::default())
//             .expect("failed to create basic trail");
//         writer
//             .write_all(&0u32.to_ne_bytes())
//             .expect("failed to write version");
//         writer
//             .write_all(&15u32.to_ne_bytes())
//             .expect("failed to write mapid ");
//         writer
//             .write_all(bytemuck::cast_slice(&[0f32; 3]))
//             .expect("failed to write first node");
//         // done
//         writer
//             .finish()
//             .expect("failed to finalize zip")
//             .into_inner()
//     }

//     #[fixture]
//     fn test_file_entries(test_zip: &[u8]) -> IndexMap<Arc<String>, Vec<u8>> {
//         let file_entries = super::read_files_from_zip(test_zip).expect("failed to deserialize");
//         assert_eq!(file_entries.len(), 3);
//         let test_xml = std::str::from_utf8(
//             file_entries
//                 .get(String::new("category.xml"))
//                 .expect("failed to get category.xml"),
//         )
//         .expect("failed to get str from category.xml contents");
//         assert_eq!(test_xml, TEST_XML);
//         let test_marker_png = file_entries
//             .get(String::new("marker.png"))
//             .expect("failed to get marker.png");
//         assert_eq!(test_marker_png, MARKER_PNG);
//         file_entries
//     }
//     #[fixture]
//     #[once]
//     fn test_pack(test_file_entries: IndexMap<Arc<String>, Vec<u8>>) -> Pack {
//         let (pack, failures) = zpack_from_xml_entries(test_file_entries, Version::new(0, 0, 0));
//         assert!(failures.errors.is_empty() && failures.warnings.is_empty());
//         assert_eq!(pack.tbins.len(), 1);
//         assert_eq!(pack.textures.len(), 1);
//         assert_eq!(
//             pack.textures
//                 .get(String::new(TEST_MARKER_PNG_NAME))
//                 .expect("failed to get marker.png from textures"),
//             MARKER_PNG
//         );

//         let tbin = pack
//             .tbins
//             .get(String::new(TEST_TRL_NAME))
//             .expect("failed to get basic trail")
//             .clone();

//         assert_eq!(tbin.nodes[0], [0.0f32; 3].into());
//         pack
//     }

//     // #[rstest]
//     // fn test_tag(test_pack: &Pack) {
//     //     let mut test_category_menu = CategoryMenu::default();
//     //     let parent_path = String::new("parent");
//     //     let child1_path = String::new("parent/child1");
//     //     let subchild_path = String::new("parent/child1/subchild");
//     //     let child2_path = String::new("parent/child2");
//     //     test_category_menu.create_category(subchild_path);
//     //     test_category_menu.create_category(child2_path);
//     //     test_category_menu.set_display_name(parent_path, "Parent".to_string());
//     //     test_category_menu.set_display_name(child1_path, "Child 1".to_string());
//     //     test_category_menu.set_display_name(subchild_path, "Sub Child".to_string());
//     //     test_category_menu.set_display_name(child2_path, "Child 2".to_string());

//     //     assert_eq!(test_category_menu, test_pack.category_menu)
//     // }

//     #[rstest]
//     fn test_markers(test_pack: &Pack) {
//         let marker = test_pack
//             .markers
//             .values()
//             .next()
//             .expect("failed to get queensdale mapdata");
//         assert_eq!(
//             marker.props.texture.as_ref().unwrap(),
//             String::new(TEST_MARKER_PNG_NAME)
//         );
//         assert_eq!(marker.position, [INCHES_PER_METER; 3].into());
//     }
//     #[rstest]
//     fn test_trails(test_pack: &Pack) {
//         let trail = test_pack
//             .trails
//             .values()
//             .next()
//             .expect("failed to get queensdale mapdata");
//         assert_eq!(
//             trail.props.tbin.as_ref().unwrap(),
//             String::new(TEST_TRL_NAME)
//         );
//         assert_eq!(
//             trail.props.trail_texture.as_ref().unwrap(),
//             String::new(TEST_MARKER_PNG_NAME)
//         );
//     }
// }
