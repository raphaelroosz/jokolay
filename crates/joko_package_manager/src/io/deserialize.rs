use crate::BASE64_ENGINE;
use base64::Engine;
use joko_core::{serde_glam::Vec3, RelativePath};
use joko_package_models::{
    attributes::{CommonAttributes, XotAttributeNameIDs},
    category::{prefix_parent, Category, RawCategory},
    marker::Marker,
    package::{PackCore, PackageImportReport},
    route::Route,
    trail::{TBin, TBinStatus, Trail},
};
use ordered_hash_map::OrderedHashMap;
use std::{
    collections::VecDeque,
    io::{Cursor, Read},
    path::Path,
    str::FromStr,
};
use tracing::{debug, error, info, info_span, instrument, trace, warn};
use uuid::Uuid;
use xot::{Element, Node, Xot};
use zip::result::{ZipError, ZipResult};

const MAX_TRAIL_CHUNK_LENGTH: f32 = 400.0;

pub(crate) fn load_pack_core_from_normalized_folder(
    core_path: &Path,
    import_report: Option<PackageImportReport>,
) -> Result<PackCore, String> {
    //called from already parsed data
    let mut core_pack = PackCore::new();
    if let Some(mut import_report) = import_report {
        import_report.reset_counters();
        import_report.uuid = core_pack.uuid;
        core_pack.report = import_report;
    }
    // walks the directory and loads all files into the hashmap
    let start = std::time::SystemTime::now();
    recursive_walk_dir_and_read_images_and_tbins(
        core_path,
        &mut core_pack,
        &RelativePath::default(),
    )
    .or(Err("failed to walk dir when loading a markerpack"))?;
    let elaspsed = start.elapsed().unwrap_or_default();
    tracing::info!(
        "Loading of core package textures from disk took {} ms",
        elaspsed.as_millis()
    );

    //categories are required to register other objects
    let cats_xml = std::fs::read_to_string(core_path.join("categories.xml"))
        .or(Err("failed to read categories.xml"))?;
    let categories_file = String::from("categories.xml");
    let parse_categories_file_start = std::time::SystemTime::now();
    parse_categories_from_normalized_file(&categories_file, &cats_xml, &mut core_pack)
        .or(Err("failed to parse category file"))?;
    let elapsed = parse_categories_file_start.elapsed().unwrap_or_default();
    info!("parse_categories_file took {} ms", elapsed.as_millis());

    // parse map data of the pack
    for entry in std::fs::read_dir(core_path).or(Err("failed to read entries of pack dir"))? {
        let dir_entry = entry.or(Err("entry error whiel reading xml files"))?;

        let name = dir_entry
            .file_name()
            .into_string()
            .or(Err("map data entry name not utf-8"))?;

        if name.ends_with(".xml") {
            if let Some(name_as_str) = name.strip_suffix(".xml") {
                match name_as_str {
                    "categories" => {
                        //already done
                    }
                    file_name => {
                        // parse map file
                        let span_guard = info_span!("load file", file_name).entered();
                        //let mut partial_pack = PackCore::partial(&core_pack.all_categories);
                        load_xml_from_normalized_file(
                            file_name,
                            &dir_entry.path(),
                            &mut core_pack,
                        )?;
                        //core_pack.merge_partial(partial_pack);
                        std::mem::drop(span_guard);
                    }
                }
            }
        } else {
            trace!("file ignored: {name}")
        }
    }
    info!(
        "Entities registered (category + markers): {}",
        core_pack.entities_parents.len()
    );
    info!("Categories registered: {}", core_pack.all_categories.len());
    info!(
        "Markers registered: {}",
        core_pack.entities_parents.len() - core_pack.all_categories.len()
    );
    info!("Maps registered: {}", core_pack.maps.len());
    info!("Textures registered: {}", core_pack.textures.len());
    info!("Trail binaries registered: {}", core_pack.tbins.len());
    Ok(core_pack)
}

fn recursive_walk_dir_and_read_images_and_tbins(
    core_path: &Path,
    pack: &mut PackCore,
    parent_path: &RelativePath,
) -> Result<(), String> {
    for entry in std::fs::read_dir(core_path).or(Err("failed to get directory entries"))? {
        let entry = entry.or(Err("dir entry error when iterating dir entries"))?;
        let name = entry
            .file_name()
            .into_string()
            .or(Err("No file name found"))?;
        let path = parent_path.join_str(&name);

        if entry
            .file_type()
            .or(Err("failed to get file type"))?
            .is_file()
        {
            if path.ends_with(".png") || path.ends_with(".trl") {
                let bytes = std::fs::read(entry.path()).or(Err("failed to read file contents"))?;
                if name.ends_with(".png") {
                    pack.register_texture(name, &path, bytes);
                } else if name.ends_with(".trl") {
                    if let Some(tbs) = parse_tbin_from_slice(&bytes) {
                        /*let is_closed: bool = tbs.closed;
                        if is_closed {
                            if tbs.iso_x {}
                            if tbs.iso_y {}
                            if tbs.iso_z {}
                        }*/
                        pack.tbins.insert(path, tbs.tbin);
                    } else {
                        info!("invalid tbin: {path}");
                    }
                }
            }
        } else {
            recursive_walk_dir_and_read_images_and_tbins(&entry.path(), pack, &path)?;
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

    let zero = glam::Vec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    // this will either be empty vec or series of vec3s.
    let nodes: VecDeque<glam::Vec3> = bytes[8..]
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

            glam::Vec3::from_array(arr)
        })
        .collect();

    //There are zeroes in trails. Reason may be either bad trail or used as a separator for several trails in same file.
    let mut iso_x = false;
    let mut iso_y = false;
    let mut iso_z = false;
    let mut closed = false;
    let mut resulting_nodes: Vec<Vec3> = Vec::new();
    if !nodes.is_empty() {
        //at least the first exist and can be accessed
        let ref_node = nodes[0];
        let mut c_iso_x = true;
        let mut c_iso_y = true;
        let mut c_iso_z = true;
        // ensure there is not too much distance between two points, if it is the case, we do split the path in several parts
        resulting_nodes.push(Vec3(ref_node));
        for (a, b) in nodes.iter().zip(nodes.iter().skip(1)) {
            //ignore zeroes since they would be separators
            if a.distance_squared(zero) > 0.01 && b.distance_squared(zero) > 0.01 {
                let distance_to_next_point = a.distance_squared(*b);
                let mut current_cursor = distance_to_next_point;
                while current_cursor > MAX_TRAIL_CHUNK_LENGTH {
                    let c = a.lerp(*b, 1.0 - current_cursor / distance_to_next_point);
                    resulting_nodes.push(Vec3(c));
                    current_cursor -= MAX_TRAIL_CHUNK_LENGTH;
                }
            }
            resulting_nodes.push(Vec3(*b));
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
        if nodes.len() > 1 {
            // TODO: get this threshold from configuration
            closed = nodes
                .front()
                .unwrap()
                .distance(*nodes.back().unwrap())
                .abs()
                < 0.1
        }
    }
    Some(TBinStatus {
        tbin: TBin {
            map_id,
            version,
            nodes: resulting_nodes,
        },
        iso_x,
        iso_y,
        iso_z,
        closed,
    })
}

fn parse_categories(
    pack: &mut PackCore,
    tree: &Xot,
    tags: impl Iterator<Item = Node>,
    first_pass_categories: &mut OrderedHashMap<String, RawCategory>,
    names: &XotAttributeNameIDs,
    source_file_uuid: &Uuid,
) {
    //called once per file
    parse_categories_recursive(
        pack,
        tree,
        tags,
        first_pass_categories,
        names,
        None,
        source_file_uuid,
    )
}

// a recursive function to parse the marker category tree.
fn parse_categories_recursive(
    pack: &mut PackCore,
    tree: &Xot,
    tags: impl Iterator<Item = Node>,
    first_pass_categories: &mut OrderedHashMap<String, RawCategory>,
    names: &XotAttributeNameIDs,
    parent_name: Option<String>,
    source_file_uuid: &Uuid,
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
            .or(ele.get_attribute(names.capital_name))
            .unwrap_or_default()
            .to_lowercase();
        if name.is_empty() {
            continue;
        }
        let mut common_attributes = CommonAttributes::default();
        common_attributes.update_common_attributes_from_element(ele, names);
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
        let full_category_name: String = if let Some(parent_name) = &parent_name {
            format!("{}.{}", parent_name, name)
        } else {
            name.to_string()
        };
        let guid = parse_guid(names, ele);
        trace!(
            "recursive_marker_category_parser {} {} {:?}",
            name,
            guid,
            parent_name
        );
        if !first_pass_categories.contains_key(&full_category_name) {
            let mut sources: OrderedHashMap<Uuid, Uuid> = OrderedHashMap::new();
            if let Some(icon_file) = common_attributes.get_icon_file() {
                if !pack.textures.contains_key(icon_file) {
                    debug!(%icon_file, "failed to find this texture in this pack");
                    pack.found_missing_inherited_texture(
                        icon_file.as_str().to_string(),
                        full_category_name.clone(),
                        source_file_uuid,
                    );
                }
            }

            sources.insert(guid, *source_file_uuid);
            first_pass_categories.insert(
                full_category_name.clone(),
                RawCategory {
                    guid,
                    parent_name: parent_name.clone(),
                    display_name: display_name.to_string(),
                    relative_category_name: name.to_string(),
                    full_category_name: full_category_name.clone(),
                    separator,
                    default_enabled,
                    props: common_attributes,
                    sources,
                },
            );
        }
        parse_categories_recursive(
            pack,
            tree,
            tree.children(tag),
            first_pass_categories,
            names,
            Some(full_category_name),
            source_file_uuid,
        );
    }
}

fn parse_categories_from_normalized_file(
    file_name: &String,
    cats_xml_str: &str,
    pack: &mut PackCore,
) -> Result<(), String> {
    let mut tree = xot::Xot::new();
    let xot_names = XotAttributeNameIDs::register_with_xot(&mut tree);
    let root_node = tree.parse(cats_xml_str).or(Err("invalid xml"))?;

    let overlay_data_node = tree.document_element(root_node).or(Err("no doc element"))?;

    if let Some(od) = tree.element(overlay_data_node) {
        let mut categories: OrderedHashMap<Uuid, Category> = Default::default();
        if od.name() == xot_names.overlay_data {
            parse_category_categories_xml_recursive(
                file_name,
                &tree,
                tree.children(overlay_data_node),
                &mut categories,
                &xot_names,
                None,
                None,
            )?;
            trace!("loaded categories: {:?}", categories);
            pack.categories = categories;
            pack.register_categories();
        } else {
            return Err("root tag is not OverlayData".to_string());
        }
    } else {
        return Err("doc element is not element???".to_string());
    }
    Ok(())
}

fn load_xml_from_normalized_file(
    file_name: &str,
    file_path: &Path,
    target: &mut PackCore,
) -> Result<(), String> {
    let mut xml_str = String::new();
    std::fs::OpenOptions::new()
        .read(true)
        .open(file_path)
        .or(Err("failed to open xml file"))?
        .read_to_string(&mut xml_str)
        .or(Err("failed to read xml string"))?;
    //TODO: launch an async load of the file + make a priority queue to have current map first
    parse_map_xml_string(file_name, &xml_str, target)
        .or(Err(format!("error parsing file: {file_name}")))
}

fn parse_map_xml_string(
    file_name: &str,
    map_xml_str: &str,
    target: &mut PackCore,
) -> Result<(), String> {
    let mut tree = Xot::new();
    let root_node = tree.parse(map_xml_str).or(Err("invalid xml"))?;
    let names = XotAttributeNameIDs::register_with_xot(&mut tree);
    let overlay_data_node = tree
        .document_element(root_node)
        .or(Err("missing doc element"))?;

    let overlay_data_element = tree.element(overlay_data_node).ok_or("no doc ele")?;

    if overlay_data_element.name() != names.overlay_data {
        return Err("root tag is not OverlayData".to_string());
    }
    let pois = tree
        .children(overlay_data_node)
        .find(|node| match tree.element(*node) {
            Some(ele) => ele.name() == names.pois,
            None => false,
        })
        .ok_or("missing pois node")?;

    for poi_node in tree.children(pois) {
        if let Some(child_element) = tree.element(poi_node) {
            let full_category_name = child_element
                .get_attribute(names.category)
                .unwrap_or_default()
                .to_lowercase();

            let span_guard = info_span!("category", full_category_name).entered();

            let opt_source_file_uuid = Uuid::from_str(
                child_element
                    .get_attribute(names._source_file_name)
                    .unwrap_or_default(),
            );
            let source_file_uuid = if let Ok(uuid) = opt_source_file_uuid {
                uuid
            } else {
                error!("Package corrupted, invalid source file uuid");
                //return Err(miette::Report::msg("Package corrupted, invalid source file uuid"));
                Uuid::new_v4()
            };

            if let Some(source_file_name) =
                target.report.source_file_uuid_to_name(&source_file_uuid)
            {
                let source_file_name = source_file_name.clone(); // this is to bypass borrow checker which has no idea this cannot be changed
                target.register_source_file(&source_file_name);
            } else {
                println!("{:?}", source_file_uuid);
            }

            //There is no file name, only an uuid to register
            target.active_source_files.insert(source_file_uuid, true);

            if child_element.name() == names.route {
                debug!("Found a route in core pack {:?}", child_element);
                let route = parse_route(
                    &names,
                    &tree,
                    &poi_node,
                    child_element,
                    &full_category_name,
                    source_file_uuid,
                );
                if let Some(route) = route {
                    target.register_route(route)?;
                } else {
                    info!("Could not parse route {:?}", child_element);
                }
            } else {
                if full_category_name.is_empty() {
                    panic!(
                        "full_category_name is empty {:?} {:?}",
                        map_xml_str, child_element
                    );
                }
                let raw_uid = child_element.get_attribute(names.guid);
                if raw_uid.is_none() {
                    info!(
                        "This POI is either invalid or inside a Route {:?}",
                        child_element
                    );
                    span_guard.exit();
                    continue;
                }
                //FIXME: this needs to be changed for partial load
                let opt_cat_uuid = target.get_category_uuid(&full_category_name);
                if opt_cat_uuid.is_none() {
                    error!(
                        "Mandatory category missing, packge is corrupted {:?} {:?}",
                        file_name, child_element
                    );
                    return Err(format!(
                        "Mandatory category missing, packge is corrupted {:?} {:?}",
                        map_xml_str, child_element
                    ));
                }
                let category_uuid = opt_cat_uuid.unwrap(); //categories MUST exist, they have already been parsed
                let guid = raw_uid
                    .and_then(|guid| {
                        let mut buffer = [0u8; 20];
                        BASE64_ENGINE
                            .decode_slice(guid, &mut buffer)
                            .ok()
                            .and_then(|_| Uuid::from_slice(&buffer[..16]).ok())
                    })
                    .ok_or(format!("invalid guid {:?}", raw_uid))?;

                if child_element.name() == names.poi {
                    debug!("Found a POI in core pack {:?}", child_element);
                    let map_id = child_element
                        .get_attribute(names.map_id)
                        .and_then(|map_id| map_id.parse::<u32>().ok())
                        .ok_or("invalid mapid")?;

                    let xpos = child_element
                        .get_attribute(names.xpos)
                        .unwrap_or_default()
                        .parse::<f32>()
                        .or(Err("invalid x position"))?;
                    let ypos = child_element
                        .get_attribute(names.ypos)
                        .unwrap_or_default()
                        .parse::<f32>()
                        .or(Err("invalid y position"))?;
                    let zpos = child_element
                        .get_attribute(names.zpos)
                        .unwrap_or_default()
                        .parse::<f32>()
                        .or(Err("invalid z position"))?;
                    let mut ca = CommonAttributes::default();
                    ca.update_common_attributes_from_element(child_element, &names);

                    let marker = Marker {
                        position: Vec3(glam::Vec3::from_array([xpos, ypos, zpos])),
                        map_id,
                        category: full_category_name.clone(),
                        parent: *category_uuid,
                        attrs: ca,
                        guid,
                        source_file_uuid,
                    };
                    target.register_marker(full_category_name, marker)?;
                } else if child_element.name() == names.trail {
                    debug!("Found a trail in core pack {:?}", child_element);
                    let map_id = child_element
                        .get_attribute(names.map_id)
                        .and_then(|map_id| map_id.parse::<u32>().ok())
                        .ok_or("invalid mapid")?;
                    let mut ca = CommonAttributes::default();
                    ca.update_common_attributes_from_element(child_element, &names);

                    let trail = Trail {
                        category: full_category_name.clone(),
                        parent: *category_uuid,
                        map_id,
                        props: ca,
                        guid,
                        dynamic: false,
                        source_file_uuid,
                    };
                    target.register_trail(full_category_name, trail)?;
                }
            }
            span_guard.exit();
        }
    }
    Ok(())
}

// a temporary recursive function to parse the marker category tree.
fn parse_category_categories_xml_recursive(
    _file_name: &String, //meant for future implementation of source file definition for categories
    tree: &Xot,
    tags: impl Iterator<Item = Node>,
    cats: &mut OrderedHashMap<Uuid, Category>,
    names: &XotAttributeNameIDs,
    parent_uuid: Option<Uuid>,
    parent_name: Option<String>,
) -> Result<(), String> {
    for tag in tags {
        if let Some(ele) = tree.element(tag) {
            if ele.name() != names.marker_category {
                continue;
            }

            let relative_category_name = ele
                .get_attribute(names.name)
                .or(ele
                    .get_attribute(names.display_name)
                    .or(ele.get_attribute(names.capital_name)))
                .unwrap_or_default()
                .to_lowercase();
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
            trace!(
                "recursive_marker_category_parser_categories_xml {} {} {:?}",
                full_category_name,
                guid,
                parent_uuid
            );
            if display_name.is_empty() {
                if parent_name.is_some() {
                    return Err(
                        "Package is corrupted, please import it again with current version"
                            .to_string(),
                    );
                }
                parse_category_categories_xml_recursive(
                    _file_name,
                    tree,
                    tree.children(tag),
                    cats,
                    names,
                    Some(guid),
                    Some(full_category_name),
                )?;
            } else {
                let current_category = if let Some(c) = cats.get_mut(&guid) {
                    c
                } else {
                    let c = Category {
                        guid,
                        parent: parent_uuid,
                        display_name: display_name.to_string(),
                        relative_category_name: relative_category_name.to_string(),
                        full_category_name: full_category_name.clone(),
                        separator,
                        default_enabled,
                        props: ca,
                        children: Default::default(),
                    };
                    cats.insert(guid, c);
                    cats.back_mut().unwrap()
                };
                parse_category_categories_xml_recursive(
                    _file_name,
                    tree,
                    tree.children(tag),
                    &mut current_category.children,
                    names,
                    Some(guid),
                    Some(full_category_name),
                )?;
            };

            std::mem::drop(span_guard);
        } else {
            //it may be a comment, a space, anything
            //info!("In file {}, ignore node {:?}", file_name, tag);
        }
    }
    Ok(())
}

//copy of zip::ZipArchive extract, but handling the bad windows path
fn extract<P: AsRef<Path>>(
    zip_archive: &mut zip::ZipArchive<Cursor<Vec<u8>>>,
    directory: P,
) -> ZipResult<()> {
    use std::fs;
    use std::io;

    for i in 0..zip_archive.len() {
        let mut file = zip_archive.by_index(i)?;
        let filepath = file
            .enclosed_name()
            .ok_or(ZipError::InvalidArchive("Invalid file path"))?;

        let filepath = filepath
            .to_owned()
            .as_mut_os_str()
            .to_str()
            .unwrap()
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_lowercase();
        let filepath = std::path::Path::new(&filepath);
        let outpath = directory.as_ref().join(filepath);

        if file.name().replace('\\', "/").ends_with('/') {
            fs::create_dir_all(&outpath)?;
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    fs::create_dir_all(p)?;
                }
            }
            let mut outfile = fs::File::create(&outpath)?;
            io::copy(&mut file, &mut outfile)?;
        }
        // Get and Set permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Some(mode) = file.unix_mode() {
                fs::set_permissions(&outpath, fs::Permissions::from_mode(mode))?;
            }
        }
    }
    Ok(())
}
pub(crate) fn get_pack_from_taco_zip(
    input_path: std::path::PathBuf,
    extract_temporary_path: &std::path::PathBuf,
) -> Result<PackCore, String> {
    let mut taco_zip = vec![];
    std::fs::File::open(input_path)
        .or(Err("Could not open target folder"))?
        .read_to_end(&mut taco_zip)
        .or(Err("Could not read target folder"))?;

    let mut zip_archive = zip::ZipArchive::new(std::io::Cursor::new(taco_zip))
        .or(Err("failed to read zip archive"))?;
    if extract_temporary_path.exists() {
        std::fs::remove_dir_all(extract_temporary_path).or(Err("Could not purge target folder"))?;
    }
    extract(&mut zip_archive, extract_temporary_path)
        .or(Err("Could not extract archive into target folder"))?;

    _get_pack_from_taco_folder(extract_temporary_path)
}

/// This first parses all the files in a zipfile into the memory and then it will try to parse a zpack out of all the files.
/// will return error if there's an issue with zipfile.
///
/// but any other errors like invalid attributes or missing markers etc.. will just be logged.
/// the intention is "best effort" parsing and not "validating" xml marker packs.
/// we will ignore any issues like unknown attributes or xml tags. "unknown" attributes means Any attributes that jokolay doesn't parse into Zpack.

#[instrument(skip_all)]
fn _get_pack_from_taco_folder(package_path: &std::path::PathBuf) -> Result<PackCore, String> {
    let mut pack = PackCore::new();

    // file paths of different file types
    let mut images = vec![];
    let mut tbins = vec![];
    let mut xmls = vec![];
    // we collect the names first, because reading a file from zip is a mutating operation.
    // So, we can't iterate AND read the file at the same time
    for entry in walkdir::WalkDir::new(package_path).into_iter() {
        let entry = entry.or(Err("Could not walk directory"))?;
        let path_as_string = entry
            .path()
            .strip_prefix(package_path)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        if path_as_string.ends_with(".png") {
            images.push(path_as_string);
        } else if path_as_string.ends_with(".trl") {
            tbins.push(path_as_string);
        } else if path_as_string.ends_with(".xml") {
            xmls.push(path_as_string);
        } else if path_as_string.replace('\\', "/").ends_with('/') {
            // directory. so, we can silently ignore this.
        } else {
            //info!("ignoring file: {name}");
        }
    }
    xmls.sort(); //build back the intended order in folder, since zip_archive may not give the files in order.
    let start_texture_loading = std::time::SystemTime::now();
    for file_path in images {
        let span = info_span!("load image", file_path).entered();
        let relative_file_path: RelativePath = file_path.parse().unwrap();
        if let Ok(bytes) = std::fs::read(package_path.join(&file_path)) {
            match image::load_from_memory_with_format(&bytes, image::ImageFormat::Png) {
                Ok(_) => {
                    pack.register_texture(file_path, &relative_file_path, bytes);
                }
                Err(e) => {
                    info!(?e, "failed to parse image file");
                }
            }
        }
        std::mem::drop(span);
    }

    for file_path in tbins {
        let span = info_span!("load tbin", file_path).entered();
        let relative_path: RelativePath = file_path.parse().unwrap();
        if let Ok(bytes) = std::fs::read(package_path.join(&file_path)) {
            if let Some(tbs) = parse_tbin_from_slice(&bytes) {
                /*let is_closed: bool = tbs.closed;
                if is_closed {
                    if tbs.iso_x {}
                    if tbs.iso_y {}
                    if tbs.iso_z {}
                }*/
                assert!(
                    pack.tbins.insert(relative_path, tbs.tbin).is_none(),
                    "duplicate tbin file {file_path}"
                );
            } else {
                info!("failed to parse tbin from slice: {relative_path}");
            }
        } else {
            info!(file_path, "failed to read tbin from zipfile");
        }
        std::mem::drop(span);
    }
    let elapsed_texture_loading = start_texture_loading.elapsed().unwrap_or_default();
    pack.report.telemetry.texture_loading = elapsed_texture_loading.as_millis();
    tracing::info!(
        "Loading of taco package textures from disk took {} ms",
        elapsed_texture_loading.as_millis()
    );

    let span_guard_categories = info_span!("deserialize xml: categories").entered();
    let start_categories_loading = std::time::SystemTime::now();
    //first pass: categories only
    let span_guard_first_pass =
        info_span!("deserialize xml first pass: load MarkerCategory").entered();
    let mut first_pass_categories: OrderedHashMap<String, RawCategory> = Default::default();
    for source_file_name in xmls.iter() {
        let source_file_name = source_file_name.to_string();
        let span_guard =
            info_span!("deserialize xml first pass: load file", source_file_name).entered();
        let r = std::fs::read_to_string(package_path.join(&source_file_name));
        let xml_str = if r.is_ok() {
            r.unwrap()
        } else {
            info!("failed to read file from zip");
            continue;
        };
        let source_file_uuid = pack.register_source_file(&source_file_name);

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

        parse_categories(
            &mut pack,
            &tree,
            tree.children(od),
            &mut first_pass_categories,
            &names,
            &source_file_uuid,
        );
        drop(span_guard);
    }
    span_guard_first_pass.exit();
    let elaspsed_first_pass = start_categories_loading.elapsed().unwrap_or_default();
    pack.report.telemetry.categories_first_pass = elaspsed_first_pass.as_millis();

    //second pass: orphan categories
    let span_guard_second_pass =
        info_span!("deserialize xml second pass: orphan categories").entered();
    let start_categories_loading_second_pass = std::time::SystemTime::now();
    for source_file_name in xmls.iter() {
        let source_file_name = source_file_name.to_string();
        let span_guard =
            info_span!("deserialize xml second pass: load file", source_file_name).entered();
        let r = std::fs::read_to_string(package_path.join(&source_file_name));
        let xml_str = if r.is_ok() {
            r.unwrap()
        } else {
            info!("failed to read file from zip");
            continue;
        };
        let source_file_uuid = pack.register_source_file(&source_file_name);

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
                debug!("missing overlay data tag");
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
                debug!("missing pois tag");
                continue;
            }
        };

        for child_node in tree.children(pois) {
            let child_element = match tree.element(child_node) {
                Some(ele) => ele,
                None => continue,
            };
            let mut full_category_name = child_element
                .get_attribute(names.category)
                .unwrap_or_default()
                .to_lowercase();
            if full_category_name.is_empty() {
                if child_element.name() == names.route {
                    // If route, take the first element inside
                    if let Some(category) =
                        parse_route_category(&names, &tree, &child_node, child_element)
                    {
                        if category.is_empty() {
                            continue;
                        }
                        full_category_name = category;
                    } else {
                        continue;
                    }
                } else {
                    continue;
                }
            }
            let guid = parse_guid(&names, child_element);
            if !pack.category_exists(&full_category_name)
                && !first_pass_categories.contains_key(&full_category_name)
            {
                let category_uuid = Uuid::new_v4();
                let mut sources: OrderedHashMap<Uuid, Uuid> = OrderedHashMap::new();
                sources.insert(guid, source_file_uuid);
                first_pass_categories.insert(
                    full_category_name.clone(),
                    RawCategory {
                        default_enabled: true,
                        guid: category_uuid,
                        parent_name: prefix_parent(&full_category_name, '.'),
                        display_name: full_category_name.clone(),
                        full_category_name: full_category_name.clone(),
                        relative_category_name: full_category_name.clone(),
                        props: Default::default(),
                        separator: false,
                        sources,
                    },
                );
                debug!(
                    "There is an orphan missing category '{}' which was created",
                    full_category_name
                );
            } else {
                let cat = first_pass_categories.get_mut(&full_category_name);
                cat.unwrap().sources.insert(guid, source_file_uuid);
            }
        }
        drop(span_guard);
    }
    span_guard_second_pass.exit();

    let elaspsed_second_pass = start_categories_loading_second_pass
        .elapsed()
        .unwrap_or_default();
    pack.report.telemetry.categories_second_pass = elaspsed_second_pass.as_millis();

    let start_categories_reassemble = std::time::SystemTime::now();
    pack.categories = Category::reassemble(&first_pass_categories, &mut pack.report);
    let elaspsed_reassemble = start_categories_reassemble.elapsed().unwrap_or_default();
    pack.report.telemetry.categories_reassemble.total = elaspsed_reassemble.as_millis();

    let start_categories_registering = std::time::SystemTime::now();
    pack.register_categories();
    let elaspsed_categories_registering =
        start_categories_registering.elapsed().unwrap_or_default();
    pack.report.telemetry.categories_registering = elaspsed_categories_registering.as_millis();

    let elaspsed = start_categories_loading.elapsed().unwrap_or_default();
    tracing::info!(
        "Loading of taco package categories from disk took {} ms, {} + {} + {}",
        elaspsed.as_millis(),
        elaspsed_first_pass.as_millis(),
        elaspsed_second_pass.as_millis(),
        elaspsed_reassemble.as_millis(),
    );

    //third and last pass: elements
    let span_guard_third_pass = info_span!("deserialize xml third pass: load elements").entered();
    let start_elements_registering = std::time::SystemTime::now();
    for source_file_name in xmls.iter() {
        let source_file_name = source_file_name.to_string();
        let span_guard =
            info_span!("deserialize xml third pass load file ", source_file_name).entered();
        let r = std::fs::read_to_string(package_path.join(&source_file_name));
        let xml_str = if r.is_ok() {
            r.unwrap()
        } else {
            info!("failed to read file from zip");
            continue;
        };
        let source_file_uuid = pack.register_source_file(&source_file_name);

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
                debug!("missing POIs tag");
                continue;
            }
        };

        for child_node in tree.children(pois) {
            let child_element = match tree.element(child_node) {
                Some(ele) => ele,
                None => continue,
            };
            let full_category_name = child_element
                .get_attribute(names.category)
                .unwrap_or_default()
                .to_lowercase();

            debug!("import element: {:?}", child_element);
            if child_element.name() == names.route {
                let route = parse_route(
                    &names,
                    &tree,
                    &child_node,
                    child_element,
                    &full_category_name,
                    source_file_uuid,
                );
                if let Some(mut route) = route {
                    //one must not create category anymore
                    route.parent = *pack.get_category_uuid(&route.category).unwrap();
                    pack.register_route(route)?;
                } else {
                    info!("Could not parse route {:?}", child_element);
                }
            } else {
                if full_category_name.is_empty() {
                    info!("full_category_name is empty {:?}", child_element);
                    continue;
                }
                if !pack.category_exists(&full_category_name) {
                    panic!(
                        "Missing category {}, previous pass should have taken care of this",
                        full_category_name
                    );
                }
                let guid = parse_guid(&names, child_element);
                let category_uuid =
                    pack.get_or_create_category_uuid(&full_category_name, guid, &source_file_uuid);
                if child_element.name() == names.poi {
                    if let Some(marker) = parse_marker(
                        &mut pack,
                        &names,
                        child_element,
                        guid,
                        &full_category_name,
                        &category_uuid,
                        source_file_uuid,
                    ) {
                        pack.register_marker(full_category_name, marker)?;
                    } else {
                        debug!("Could not parse POI");
                    }
                } else if child_element.name() == names.trail {
                    if let Some(trail) = parse_trail(
                        &mut pack,
                        &names,
                        child_element,
                        guid,
                        &full_category_name,
                        &category_uuid,
                        source_file_uuid,
                    ) {
                        pack.register_trail(full_category_name, trail)?;
                    } else {
                        debug!("Could not parse Trail");
                    }
                } else {
                    info!("unknown element: {:?}", child_element);
                }
            }
        }

        drop(span_guard);
    }
    span_guard_third_pass.exit();
    span_guard_categories.exit();
    let elaspsed_elements_registering = start_elements_registering.elapsed().unwrap_or_default();
    pack.report.telemetry.elements_registering = elaspsed_elements_registering.as_millis();

    let elapsed_import = start_texture_loading.elapsed().unwrap_or_default();
    pack.report.telemetry.total = elapsed_import.as_millis();
    Ok(pack)
}

fn parse_optional_guid(names: &XotAttributeNameIDs, child: &Element) -> Option<Uuid> {
    child.get_attribute(names.guid).and_then(|guid| {
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
}
fn parse_guid(names: &XotAttributeNameIDs, child: &Element) -> Uuid {
    parse_optional_guid(names, child).unwrap_or_else(Uuid::new_v4)
}

fn parse_marker(
    pack: &mut PackCore,
    names: &XotAttributeNameIDs,
    poi_element: &Element,
    guid: Uuid,
    category_name: &str,
    category_uuid: &Uuid,
    source_file_uuid: Uuid,
) -> Option<Marker> {
    let mut common_attributes = CommonAttributes::default();
    common_attributes.update_common_attributes_from_element(poi_element, names);
    if let Some(icon_file) = common_attributes.get_icon_file() {
        if !pack.textures.contains_key(icon_file) {
            debug!(%icon_file, "failed to find this texture in this pack");
            pack.found_missing_element_texture(
                icon_file.as_str().to_string(),
                guid,
                &source_file_uuid,
            );
        }
    } else if let Some(icf) = poi_element.get_attribute(names.icon_file) {
        debug!(icf, "marker's icon file attribute failed to parse");
        pack.found_missing_element_texture(icf.to_string(), guid, &source_file_uuid);
    }

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
        Some(Marker {
            position: Vec3(glam::Vec3::from_array([xpos, ypos, zpos])),
            map_id,
            category: category_name.to_owned(),
            parent: *category_uuid,
            attrs: common_attributes,
            guid,
            source_file_uuid,
        })
    } else {
        debug!("missing map id");
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
    Vec3(glam::Vec3 { x, y, z })
}

fn parse_route_category(
    names: &XotAttributeNameIDs,
    tree: &Xot,
    route_node: &Node,
    route_element: &Element,
) -> Option<String> {
    for child_node in tree.children(*route_node) {
        let child = match tree.element(child_node) {
            Some(ele) => ele,
            None => continue,
        };
        if child.name() == names.poi {
            if let Some(cat) = child.get_attribute(names.category) {
                return Some(cat.to_string());
            }
        }
    }
    info!("Could not find a category for route element: {route_element:?}");
    None
}

fn parse_route(
    names: &XotAttributeNameIDs,
    tree: &Xot,
    route_node: &Node,
    route_element: &Element,
    category_name: &str,
    source_file_uuid: Uuid,
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
    let reset_position = glam::Vec3::new(resetposx, resetposy, resetposz);
    let reset_range = route_element
        .get_attribute(names.reset_range)
        .and_then(|map_id| map_id.parse::<f64>().ok());
    let name = route_element
        .get_attribute(names.name)
        .or(route_element.get_attribute(names.capital_name));

    if name.is_none() {
        info!("route element is missing name: {route_element:?}");
        return None;
    }
    let mut category: String = category_name.to_owned();
    let mut category_uuid: Option<Uuid> = parse_optional_guid(names, route_element);
    let mut map_id: Option<u32> = route_element
        .get_attribute(names.map_id)
        .and_then(|map_id| map_id.parse::<u32>().ok());
    for child_node in tree.children(*route_node) {
        let child = match tree.element(child_node) {
            Some(ele) => ele,
            None => continue,
        };
        if child.name() == names.poi {
            let marker = parse_position(names, child);
            path.push(marker);
            if category.is_empty() {
                if let Some(cat) = child.get_attribute(names.category) {
                    category = cat.to_string();
                }
            }
            if category_uuid.is_none() {
                category_uuid = parse_optional_guid(names, child)
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
    if category_uuid.is_none() {
        info!("Could not find a uuid for route element: {route_element:?}");
        return None;
    }
    debug!(
        "found route with {:?} elements {route_element:?}",
        path.len()
    );

    Some(Route {
        category,
        parent: category_uuid.unwrap(),
        path,
        reset_position: Vec3(reset_position),
        reset_range: reset_range.unwrap_or(0.0),
        map_id: map_id.unwrap(),
        name: name.unwrap().into(),
        guid: parse_guid(names, route_element),
        source_file_uuid,
    })
}

fn parse_trail(
    pack: &mut PackCore,
    names: &XotAttributeNameIDs,
    trail_element: &Element,
    guid: Uuid,
    category_name: &str,
    category_uuid: &Uuid,
    source_file_uuid: Uuid,
) -> Option<Trail> {
    //http://www.gw2taco.com/2022/04/a-proper-marker-editor-finally.html

    let mut common_attributes = CommonAttributes::default();
    common_attributes.update_common_attributes_from_element(trail_element, names);

    if let Some(tex) = common_attributes.get_texture() {
        if !pack.textures.contains_key(tex) {
            info!(%tex, "failed to find this texture in this pack");
            pack.found_missing_element_texture(tex.as_str().to_string(), guid, &source_file_uuid);
        }
    }

    #[allow(clippy::manual_map)]
    // This is not exactly a manual map, we register something more in pack on some condition: a missing trail.
    if let Some(map_id) = trail_element
        .get_attribute(names.trail_data)
        .and_then(|trail_data| {
            //fix the path which may be a mix of windows and linux path
            let file_path: RelativePath = trail_data.parse().unwrap();
            if let Some(tb) = pack.tbins.get(&file_path) {
                Some(tb.map_id)
            } else {
                pack.found_missing_trail(&file_path, guid, &source_file_uuid);
                None
            }
        })
    {
        Some(Trail {
            category: category_name.to_owned(),
            parent: *category_uuid,
            map_id,
            props: common_attributes,
            guid,
            dynamic: false,
            source_file_uuid,
        })
    } else {
        /*let td = trail_element.get_attribute(names.trail_data);
        let file_path: RelativePath = td.unwrap_or_default().parse().unwrap();
        //pack.report.found_orphan_trail(&file_path, guid, &source_file_name);
        let tbin = pack.tbins.get(&file_path).map(|tbin| (tbin.map_id, tbin.version));
        info!("missing map_id: {td:?} {file_path} {tbin:?}");
        */
        None
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
