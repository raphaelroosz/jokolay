use joko_core::RelativePath;
use tracing::{debug, trace};
use uuid::Uuid;
use std::collections::{HashMap, HashSet, BTreeMap};
use indexmap::IndexMap;
use crate::marker::Marker;
use crate::route::{route_to_tbin, route_to_trail, Route};
use crate::trail::{TBin, Trail};
use crate::category::{prefix_until_nth_char, Category};
use crate::map::MapData;


#[derive(Debug, Clone)]
pub struct PackCore {
    /*
        PackCore is a temporary holder of data
        It is moved and breaked down into a Data and Texture part. Former for background work and later for UI display.
    */
    pub uuid: Uuid,
    pub textures: HashMap<RelativePath, Vec<u8>>,
    pub tbins: HashMap<RelativePath, TBin>,
    pub categories: IndexMap<Uuid, Category>,
    pub all_categories: HashMap<String, Uuid>,
    pub late_discovery_categories: HashSet<Uuid>,//categories that are defined only from a marker point of view. It needs to be saved in some way or it's lost at next start.
    pub entities_parents: HashMap<Uuid, Uuid>,
    pub source_files: BTreeMap<String, bool>,//TODO: have a reference containing pack name and maybe even path inside the package
    pub maps: HashMap<u32, MapData>,
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

    pub fn register_marker(&mut self, full_category_name: String, mut marker: Marker) -> Result<(), miette::Error> {
        let uuid_to_insert = self.register_uuid(&full_category_name, &marker.guid)?;
        marker.guid = uuid_to_insert;
        if !self.maps.contains_key(&marker.map_id) {
            self.maps.insert(marker.map_id, MapData::default());
        }
        self.maps.get_mut(&marker.map_id).unwrap().markers.insert(uuid_to_insert, marker);
        Ok(())
    }

    pub fn register_trail(&mut self, full_category_name: String, mut trail: Trail) -> Result<(), miette::Error> {
        let uuid_to_insert = self.register_uuid(&full_category_name, &trail.guid)?;
        trail.guid = uuid_to_insert;
        if !self.maps.contains_key(&trail.map_id) {
            self.maps.insert(trail.map_id, MapData::default());
        }
        self.maps.get_mut(&trail.map_id).unwrap().trails.insert(uuid_to_insert, trail);
        Ok(())
    }

    pub fn register_route(&mut self, mut route: Route) -> Result<(), miette::Error> {
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