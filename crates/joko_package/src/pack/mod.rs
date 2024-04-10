mod common;
mod marker;
mod trail;
mod route;

use std::collections::{BTreeMap, HashMap, HashSet};

use indexmap::IndexMap;
use ordered_hash_map::OrderedHashMap;

use tracing::{debug, info, trace};

use joko_core::RelativePath;
pub use common::*;
pub(crate) use marker::*;
pub(crate) use trail::*;
pub(crate) use route::*;
use uuid::Uuid;

#[derive(Default, Debug, Clone)]
pub(crate) struct MapData {
    pub markers: IndexMap<Uuid, Marker>,
    pub routes: IndexMap<Uuid, Route>,
    pub trails: IndexMap<Uuid, Trail>,
}

#[derive(Debug, Clone)]
pub(crate) struct RawCategory {
    pub guid: Uuid,
    pub parent_name: Option<String>,
    pub display_name: String,
    pub relative_category_name: String,
    pub full_category_name: String,
    pub separator: bool,
    pub default_enabled: bool,
    pub props: CommonAttributes,
}

#[derive(Debug, Clone)]
pub(crate) struct Category {
    pub guid: Uuid,
    pub parent: Option<Uuid>,
    pub display_name: String,
    pub relative_category_name: String,
    pub full_category_name: String,
    pub separator: bool,
    pub default_enabled: bool,
    pub props: CommonAttributes,
    pub children: IndexMap<Uuid, Category>,
}

#[derive(Debug, Clone)]
pub struct PackCore {
    /*
        PackCore is a temporary holder of data
        It is moved and breaked down into a Data and Texture part. Former for background work and later for UI display.
    */
    pub uuid: Uuid,
    pub textures: HashMap<RelativePath, Vec<u8>>,
    pub(crate) tbins: HashMap<RelativePath, TBin>,
    pub(crate) categories: IndexMap<Uuid, Category>,
    pub all_categories: HashMap<String, Uuid>,
    pub late_discovery_categories: HashSet<Uuid>,//categories that are defined only from a marker point of view. It needs to be saved in some way or it's lost at next start.
    pub entities_parents: HashMap<Uuid, Uuid>,
    pub source_files: BTreeMap<String, bool>,//TODO: have a reference containing pack name and maybe even path inside the package
    pub maps: HashMap<u32, MapData>,
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

impl PackCore {

    pub fn new() -> Self {
        let mut res = Self {
            all_categories: Default::default(),
            categories: Default::default(),
            entities_parents: Default::default(),
            late_discovery_categories: Default::default(),
            maps: Default::default(),
            source_files: Default::default(),
            tbins: Default::default(),
            textures: Default::default(),
            uuid: Default::default(),
        };
        res.uuid = Uuid::new_v4();
        res
    }
    pub fn partial(all_categories: &HashMap<String, Uuid>) -> Self {
        // When loading extra data, one MUST know ALL the already existing categories. None MUST be missing.
        let mut res: Self =  Self::new();
        res.all_categories = all_categories.clone();
        res
    }

    pub fn merge_partial(&mut self, partial_pack: PackCore) {
        self.maps.extend(partial_pack.maps);
        self.all_categories = partial_pack.all_categories;
        self.late_discovery_categories.extend(partial_pack.late_discovery_categories);
        self.source_files.extend(partial_pack.source_files);
        self.tbins.extend(partial_pack.tbins);
        self.entities_parents.extend(partial_pack.entities_parents);
    }
    pub fn category_exists(&self, full_category_name: &String) -> bool {
        self.all_categories.contains_key(full_category_name)
    }
    
    pub fn get_category_uuid(&self, full_category_name: &String) -> Option<&Uuid> {
        self.all_categories.get(full_category_name)
    }

    pub fn get_or_create_category_uuid(&mut self, full_category_name: &String) -> Uuid {
        if let Some(category_uuid) = self.all_categories.get(full_category_name) {
            category_uuid.clone()
        } else {
            //TODO: if import is "dirty", create missing category
            //TODO: default import mode is "strict" (get inspiration from HTML modes)
            debug!("There is no defined category for {}", full_category_name);

            let mut n = 0;
            let mut last_uuid: Option<Uuid> = None;
            while let Some(parent_full_category_name) = prefix_until_nth_char(&full_category_name, '.', n) {
                n += 1;
                if let Some(parent_uuid) = self.all_categories.get(&parent_full_category_name) {
                    //FIXME: might want to make the difference between impacted parents and actual missing category
                    self.late_discovery_categories.insert(*parent_uuid);
                    last_uuid = Some(*parent_uuid);
                } else {
                    let new_uuid = Uuid::new_v4();
                    debug!("Partial create missing parent category: {} {}", parent_full_category_name, new_uuid);
                    self.all_categories.insert(parent_full_category_name.clone(), new_uuid);
                    self.late_discovery_categories.insert(new_uuid);
                    last_uuid = Some(new_uuid);
                }
            }
            trace!("{} uuid: {:?}", full_category_name, last_uuid);
            assert!(last_uuid.is_some());
            last_uuid.unwrap()
        }
    }

    pub fn register_uuid(&mut self, full_category_name: &String, uuid: &Uuid) -> Result<Uuid, miette::Error>{
        if let Some(parent_uuid) = self.all_categories.get(full_category_name) {
            let mut uuid_to_insert = uuid.clone();
            while self.entities_parents.contains_key(&uuid_to_insert) {
                trace!("Uuid collision detected {} for elements in {}", uuid_to_insert, full_category_name);
                uuid_to_insert = Uuid::new_v4();
            }
            self.entities_parents.insert(uuid_to_insert, *parent_uuid);
            Ok(uuid_to_insert)
        } else {
            //FIXME: this means a broken package, we could fix it by making usage of the relative category the node is in.
            Err(miette::Error::msg(format!("Can't register world entity {} {}, no associated category found.", full_category_name, uuid)))
        }
    }

    pub(crate) fn register_marker(&mut self, full_category_name: String, mut marker: Marker) -> Result<(), miette::Error> {
        let uuid_to_insert = self.register_uuid(&full_category_name, &marker.guid)?;
        marker.guid = uuid_to_insert;
        if !self.maps.contains_key(&marker.map_id) {
            self.maps.insert(marker.map_id, MapData::default());
        }
        self.maps.get_mut(&marker.map_id).unwrap().markers.insert(uuid_to_insert, marker);
        Ok(())
    }

    pub(crate) fn register_trail(&mut self, full_category_name: String, mut trail: Trail) -> Result<(), miette::Error> {
        let uuid_to_insert = self.register_uuid(&full_category_name, &trail.guid)?;
        trail.guid = uuid_to_insert;
        if !self.maps.contains_key(&trail.map_id) {
            self.maps.insert(trail.map_id, MapData::default());
        }
        self.maps.get_mut(&trail.map_id).unwrap().trails.insert(uuid_to_insert, trail);
        Ok(())
    }

    pub(crate) fn register_route(&mut self, mut route: Route) -> Result<(), miette::Error> {
        let file_name = format!("data/dynamic_trails/{}.trl", &route.guid);
        let tbin_path: RelativePath = file_name.parse().unwrap();
        let uuid_to_insert = self.register_uuid(&route.category, &route.guid)?;
        route.guid = uuid_to_insert;
        let trail = route_to_trail(&route, &tbin_path);
        let tbin = route_to_tbin(&route);

        self.tbins.insert(tbin_path, tbin);//there may be duplicates since we load and save each time
        if !self.maps.contains_key(&trail.map_id) {
            self.maps.insert(trail.map_id, MapData::default());
        }
        self.maps.get_mut(&trail.map_id).unwrap().trails.insert(uuid_to_insert, trail);
        self.maps.get_mut(&route.map_id).unwrap().routes.insert(uuid_to_insert, route);
        Ok(())
    }
    
    pub fn register_categories(&mut self) {
        let mut entities_parents: HashMap<Uuid, Uuid> = Default::default();
        let mut all_categories: HashMap<String, Uuid> = Default::default();
        Self::recursive_register_categories(&mut entities_parents, &self.categories, &mut all_categories);
        self.entities_parents.extend(entities_parents);
        self.all_categories = all_categories;
    }
    fn recursive_register_categories(
        entities_parents: &mut HashMap<Uuid, Uuid>, 
        categories: &IndexMap<Uuid, Category>, 
        all_categories: &mut HashMap<String, Uuid>,
    ) {
        for (_, cat) in categories.iter() {
            debug!("Register category {} {} {:?}", cat.full_category_name, cat.guid, cat.parent);
            all_categories.insert(cat.full_category_name.clone(), cat.guid);
            if let Some(parent) = cat.parent {
                entities_parents.insert(cat.guid, parent);
            }
            Self::recursive_register_categories(entities_parents, &cat.children, all_categories);
        }
    }
}


pub fn prefix_until_nth_char(s: &str, pat: char, n: usize) -> Option<String> {
    let res = s.match_indices(pat)
        .nth(n)
        .map(|(index, _)| s.split_at(index))
        .map(|(left, _)| left.to_string());
    debug!("prefix_until_nth_char {} {} {:?}", s, n, res);
    res
}

pub fn nth_chunk(s: &str, pat: char, n: usize) -> String {
    let nb_matches = s.matches(pat).count();
    assert!(nb_matches + 1 > n);
    let res = s.split(pat)
        .nth(n)
    ;
    debug!("nth_chunk {} {} {:?}", s, n, res);
    res.unwrap().to_string()
}

pub fn prefix_parent(s: &str, pat: char) -> Option<String> {
    let n = s.matches(pat).count();
    assert!(n > 0);
    let res = s.match_indices(pat)
        .nth(n - 1)
        .map(|(index, _)| s.split_at(index))
        .map(|(left, _)| left.to_string());
    debug!("prefix_parent {} {} {:?}", s, n, res);
    res
}

impl Category {
    // Required method
    pub fn from(value: &RawCategory, parent: Option<Uuid>) -> Self {
        Self {
            guid: value.guid.clone(),
            props: value.props.clone(),
            separator: value.separator,
            default_enabled: value.default_enabled,
            display_name: value.display_name.clone(),
            relative_category_name: value.relative_category_name.clone(),
            full_category_name: value.full_category_name.clone(),
            parent: parent,
            children: Default::default()
        }
    }
    pub fn per_uuid<'a>(categories: &'a mut IndexMap<Uuid, Category>, uuid: &Uuid, depth: usize) -> Option<&'a mut Category> {
        for (_, cat) in categories {
            if &cat.guid == uuid {
                return Some(cat);
            }
            let sub_res = Category::per_uuid(&mut cat.children, uuid, depth + 1);
            if sub_res.is_some() {
                return sub_res;
            }
        }
        return None;
    }
    pub fn reassemble(
        input_first_pass_categories: &OrderedHashMap<String, RawCategory>,
        late_discovered_categories: &mut HashSet<Uuid>,
    ) -> IndexMap<Uuid, Category> {
        let mut first_pass_categories = input_first_pass_categories.clone();
        let mut second_pass_categories: OrderedHashMap<String, RawCategory> = Default::default();
        let mut need_a_pass: bool = true;
    
        let mut third_pass_categories: IndexMap<Uuid, Category> = Default::default();
        let mut third_pass_categories_ref: Vec<Uuid> = Default::default();
        let mut root: IndexMap<Uuid, Category> = Default::default();
        while need_a_pass {
            need_a_pass = false;
            for (key, value) in first_pass_categories.iter() {
                debug!("reassemble_categories {:?}", value);
                let mut to_insert = value.clone();
                if value.relative_category_name.matches('.').count() > 0  && value.relative_category_name == value.full_category_name {
                    let mut n = 0;
                    let mut last_name: Option<String> = None;
                    // This is an almost duplication of code of pack/mod.rs
                    while let Some(parent_name) = prefix_until_nth_char(&value.relative_category_name, '.', n) {
                        debug!("{} {}", parent_name, n);
                        if let Some(parent_category) = first_pass_categories.get(&parent_name) {
                            late_discovered_categories.insert(parent_category.guid);
                            last_name = Some(parent_name.clone());
                        } else if let Some(parent_category) = second_pass_categories.get(&parent_name) {
                            late_discovered_categories.insert(parent_category.guid);
                            last_name = Some(parent_name.clone());
                        }else{
                            let new_uuid = Uuid::new_v4();
                            let relative_category_name = nth_chunk(&value.relative_category_name, '.', n);
                            debug!("reassemble_categories Partial create missing parent category: {} {} {} {}", parent_name, relative_category_name, n, new_uuid);
                            let to_insert = RawCategory {
                                default_enabled: value.default_enabled,
                                guid: new_uuid,
                                relative_category_name: relative_category_name.clone(),
                                display_name: relative_category_name.clone(),
                                parent_name: prefix_until_nth_char(&parent_name, '.', n-1),
                                props: value.props.clone(),
                                separator: false,
                                full_category_name: parent_name.clone()
                            };
                            last_name = Some(to_insert.full_category_name.clone());
                            second_pass_categories.insert(parent_name.clone(), to_insert);
                            late_discovered_categories.insert(new_uuid);
                            need_a_pass = true;
                        }
                        n += 1;
                    }
                    late_discovered_categories.insert(value.guid);
                    to_insert.relative_category_name = nth_chunk(&value.relative_category_name, '.', n);
                    to_insert.display_name = to_insert.relative_category_name.clone();
                    debug!("parent_name: {:?}, new name: {}, old name: {}", last_name, to_insert.relative_category_name, &value.relative_category_name);
                    assert!(last_name.is_some());
                    to_insert.parent_name = last_name;
                } else {
                    to_insert.parent_name = if let Some(parent_name) = &value.parent_name {
                        if let Some(parent_category) = first_pass_categories.get(parent_name) {
                            Some(parent_category.full_category_name.clone())
                        } else {
                            None
                        }
                    }else {
                        None
                    };
                    debug!("insert as is {:?}", to_insert);
                }
                second_pass_categories.insert(key.clone(), to_insert);
            }
            if need_a_pass {
                std::mem::swap(&mut first_pass_categories, &mut second_pass_categories);
                second_pass_categories.clear();
            }
        }
        for (key, value) in second_pass_categories {
            let parent = if let Some(parent_name) = &value.parent_name {
                if let Some(parent_category) = first_pass_categories.get(parent_name) {
                    Some(parent_category.guid.clone())
                } else {
                    None
                }
            } else {
                None
            };
            
            debug!("{} parent is {:?}", key , parent);
            let cat = Category::from(&value, parent);
            let ref_uuid = cat.guid.clone();
            if third_pass_categories.insert(cat.guid.clone(), cat).is_none() {
                third_pass_categories_ref.push(ref_uuid);
            }
        }
    
        for full_category_name in third_pass_categories_ref {
            if let Some(cat) = third_pass_categories.shift_remove(&full_category_name) {
                if let Some(parent) = cat.parent {
                    if let Some(parent_category) = Category::per_uuid(&mut third_pass_categories, &parent, 0) {
                        parent_category.children.insert(cat.guid.clone(), cat);
                    } else if let Some(parent_category) = Category::per_uuid(&mut root, &parent, 0) {
                        parent_category.children.insert(cat.guid.clone(), cat);
                    } else {
                        panic!("Could not find parent {} for {:?}", parent, cat);
                    }
                } else {
                    root.insert(cat.guid.clone(), cat);
                }
            } else {
                panic!("Some bad logic at works");
            }
        }
        debug!("reassemble_categories {:?}", root);
        root
    }
    

}

