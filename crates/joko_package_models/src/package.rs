use crate::category::{prefix_until_nth_char, Category};
use crate::map::MapData;
use crate::marker::Marker;
use crate::route::{route_to_tbin, route_to_trail, Route};
use crate::trail::{TBin, Trail};
use base64::Engine;
use joko_core::RelativePath;
use ordered_hash_map::OrderedHashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::{BTreeMap, HashMap, HashSet};
use tracing::{debug, trace};
use uuid::Uuid;

pub const BASE64_ENGINE: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    base64::engine::GeneralPurposeConfig::new(),
);

fn serialize_reference<S>(reference: &ElementReference, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match reference {
        ElementReference::Uuid(uuid) => {
            let to_do = BASE64_ENGINE.encode(uuid);
            serializer.serialize_str(to_do.as_str())
        }
        ElementReference::Category(full_category_name) => {
            serializer.serialize_str(full_category_name.as_str())
        }
    }
}

fn deserialize_reference<'de, D>(deserializer: D) -> Result<ElementReference, D::Error>
where
    D: Deserializer<'de>,
{
    let encoded_uuid_or_full_category_name = String::deserialize(deserializer)?;
    if let Ok(bytes) = BASE64_ENGINE.decode(encoded_uuid_or_full_category_name.as_bytes()) {
        let mut uuid_bytes: [u8; 16] = Default::default();
        uuid_bytes.copy_from_slice(bytes.as_slice());
        let res = Uuid::from_bytes(uuid_bytes);
        Ok(ElementReference::Uuid(res))
    } else {
        Ok(ElementReference::Category(
            encoded_uuid_or_full_category_name,
        ))
    }
}

fn serialize_uuid_in_base64<S>(uuid: &Uuid, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let to_do = BASE64_ENGINE.encode(uuid);
    serializer.serialize_str(to_do.as_str())
}

fn deserialize_uuid_in_base64<'de, D>(deserializer: D) -> Result<Uuid, D::Error>
where
    D: Deserializer<'de>,
{
    let encoded = String::deserialize(deserializer)?;
    if let Ok(bytes) = BASE64_ENGINE.decode(encoded.as_bytes()) {
        let mut uuid_bytes: [u8; 16] = Default::default();
        uuid_bytes.copy_from_slice(bytes.as_slice());
        let res = Uuid::from_bytes(uuid_bytes);
        Ok(res)
    } else {
        Err(serde::de::Error::custom(
            "Could not parse base64 encoded uuid",
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageCategorySource {
    full_category_name: String,
    #[serde(
        serialize_with = "serialize_uuid_in_base64",
        deserialize_with = "deserialize_uuid_in_base64"
    )]
    requester_uuid: Uuid,
    source_file_name: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
enum ElementReference {
    Uuid(Uuid),
    Category(String),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PackageElementSource {
    file_path: String,
    #[serde(
        serialize_with = "serialize_reference",
        deserialize_with = "deserialize_reference"
    )]
    requester_reference: ElementReference,
    source_file_name: String,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PackageImportStatistics {
    pub categories: usize,         // total number of found categories
    pub missing_categories: usize, // categories that should be defined in a <MarkerCategory /> node
    pub textures: usize,           //total number of texture used (or should)
    pub missing_textures: usize,   // how many of the textures are missing
    pub entities: usize, // total number of tracked elements: categories, trails, markers, ...
    pub markers: usize,  // total number of markers
    pub trails: usize,   // total number of trails
    pub routes: usize, // total number of routes defined, they shall not count as trails even if imported as such
    pub maps: usize,   // total number of maps covered
    pub source_files: usize, // total number of XML files
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PackageImportReassembleTelemetry {
    pub total: u128,
    pub initialize: u128,
    pub missing_categories_creation: u128,
    pub parent_child_relationship: u128,
    pub tree_insertion: u128,
}
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PackageImportTelemetry {
    pub total: u128,
    pub texture_loading: u128,
    pub categories_loading: u128,
    pub categories_first_pass: u128,
    pub categories_second_pass: u128,
    pub categories_registering: u128,
    pub categories_reassemble: PackageImportReassembleTelemetry,
    pub elements_registering: u128,
}
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PackageImportReport {
    #[serde(skip)]
    pub uuid: Uuid,
    pub number_of: PackageImportStatistics, // count everything we can think of
    pub telemetry: PackageImportTelemetry,  // all the time spent in which step
    late_discovered_categories: OrderedHashMap<Uuid, String>, //categories that are defined only from a marker point of view. It needs to be saved in some way or it's lost at next start.
    missing_categories: Vec<PackageCategorySource>, //categories that are defined only from a marker point of view. It needs to be saved in some way or it's lost at next start.
    #[serde(skip)]
    _missing_categories_tracker: HashSet<String>, // for tracking purpose to avoid duplicate
    #[serde(skip)]
    _missing_textures_tracker: HashSet<String>, // for tracking purpose to avoid duplicate
    missing_textures: Vec<PackageElementSource>,    //missing texture for display
    missing_trails: Vec<PackageElementSource>,      //missing file for trail
    source_files: bimap::BiMap<String, Uuid>, //map of all files to uuid. When exporting this shall have to be reversed.
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackCore {
    /*
        PackCore is a temporary holder of data
        It is moved and breaked down into a Data and Texture part. Former for background work and later for UI display.
    */
    pub uuid: Uuid,
    pub textures: HashMap<RelativePath, Vec<u8>>,
    pub tbins: HashMap<RelativePath, TBin>,
    pub categories: OrderedHashMap<Uuid, Category>,
    pub all_categories: HashMap<String, Uuid>,
    pub entities_parents: HashMap<Uuid, Uuid>,
    pub active_source_files: BTreeMap<Uuid, bool>,
    pub maps: HashMap<u32, MapData>,
    pub report: PackageImportReport,
}

impl PackageImportReport {
    pub const REPORT_FILE_NAME: &'static str = "import_report.json";

    pub fn reset_counters(&mut self) {
        self.number_of = Default::default();
    }
    fn merge_partial(&mut self, partial_report: PackageImportReport) {
        self.late_discovered_categories
            .extend(partial_report.late_discovered_categories);
    }

    pub fn is_category_discovered_late(&self, uuid: Uuid) -> bool {
        self.late_discovered_categories.contains_key(&uuid)
    }

    pub fn source_file_uuid_to_name(&self, source_file_uuid: &Uuid) -> Option<&String> {
        self.source_files.get_by_right(source_file_uuid)
    }
    pub fn source_file_name_to_uuid(&self, source_file_name: &String) -> Option<&Uuid> {
        self.source_files.get_by_left(source_file_name)
    }

    pub fn found_category_late(&mut self, full_category_name: &str, category_uuid: Uuid) {
        self.late_discovered_categories
            .insert(category_uuid, full_category_name.to_owned());
    }
    pub fn found_category_late_with_details(
        &mut self,
        full_category_name: &String,
        category_uuid: Uuid,
        requester_uuid: &Uuid,
        source_file_uuid: &Uuid,
    ) {
        self.found_category_late(full_category_name, category_uuid);
        let source_file_name = self.source_files.get_by_right(source_file_uuid).unwrap();

        //for this to work we need to keep track of where each category was called and thus defined since late
        self.missing_categories.push(PackageCategorySource {
            full_category_name: full_category_name.clone(),
            requester_uuid: *requester_uuid,
            source_file_name: source_file_name.clone(),
        });
        if !self
            ._missing_categories_tracker
            .contains(full_category_name)
        {
            self.number_of.missing_categories += 1;
            self._missing_categories_tracker
                .insert(full_category_name.clone());
        }
    }
    fn found_missing_texture(&mut self, file_path: &String) {
        if !self._missing_textures_tracker.contains(file_path) {
            self.number_of.missing_textures += 1;
            self._missing_textures_tracker.insert(file_path.clone());
        }
    }
}

impl PackCore {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut res = Self {
            all_categories: Default::default(),
            categories: Default::default(),
            entities_parents: Default::default(),
            report: Default::default(),
            maps: Default::default(),
            active_source_files: Default::default(),
            tbins: Default::default(),
            textures: Default::default(),
            uuid: Default::default(),
        };
        res.uuid = Uuid::new_v4();
        res.report.uuid = res.uuid;
        res
    }
    pub fn partial(all_categories: &HashMap<String, Uuid>) -> Self {
        // When loading extra data, one MUST know ALL the already existing categories. None MUST be missing.
        let mut res: Self = Self::new();
        res.all_categories = all_categories.clone();
        res
    }

    pub fn merge_partial(&mut self, partial_pack: PackCore) {
        self.maps.extend(partial_pack.maps);
        self.all_categories = partial_pack.all_categories;
        self.report.merge_partial(partial_pack.report);
        self.active_source_files
            .extend(partial_pack.active_source_files);
        self.tbins.extend(partial_pack.tbins);
        self.entities_parents.extend(partial_pack.entities_parents);
    }
    pub fn category_exists(&self, full_category_name: &String) -> bool {
        self.all_categories.contains_key(full_category_name)
    }

    pub fn get_category_uuid(&self, full_category_name: &String) -> Option<&Uuid> {
        self.all_categories.get(full_category_name)
    }

    pub fn get_or_create_category_uuid(
        &mut self,
        full_category_name: &String,
        requester_uuid: Uuid,
        source_file_uuid: &Uuid,
    ) -> Uuid {
        if let Some(category_uuid) = self.all_categories.get(full_category_name) {
            *category_uuid
        } else {
            // If imported package is "dirty", create missing category
            //TODO: default import mode is "strict" (get inspiration from HTML modes)
            debug!("There is no defined category for {}", full_category_name);

            let mut n = 0;
            let mut last_uuid: Option<Uuid> = None;
            while let Some(parent_full_category_name) =
                prefix_until_nth_char(full_category_name, '.', n)
            {
                n += 1;
                if let Some(parent_uuid) = self.all_categories.get(&parent_full_category_name) {
                    //FIXME: might want to make the difference between impacted parents and actual missing category
                    self.report
                        .found_category_late(full_category_name, *parent_uuid);
                    last_uuid = Some(*parent_uuid);
                } else {
                    let new_uuid = Uuid::new_v4();
                    debug!(
                        "Partial create missing parent category: {} {}",
                        parent_full_category_name, new_uuid
                    );
                    self.all_categories
                        .insert(parent_full_category_name.clone(), new_uuid);
                    self.report.found_category_late_with_details(
                        full_category_name,
                        new_uuid,
                        &requester_uuid,
                        source_file_uuid,
                    );
                    last_uuid = Some(new_uuid);
                }
            }
            trace!("{} uuid: {:?}", full_category_name, last_uuid);
            assert!(last_uuid.is_some());
            last_uuid.unwrap()
        }
    }

    pub fn get_source_file_uuid(&mut self, source_file_name: &String) -> Uuid {
        // Must always exist when called since we registered the file already.
        *self
            .report
            .source_files
            .get_by_left(source_file_name)
            .unwrap()
    }

    pub fn register_source_file(&mut self, source_file_name: &String) -> Uuid {
        if !self.report.source_files.contains_left(source_file_name) {
            let uuid_to_insert = Uuid::new_v4(); //TODO: have a uuid built from current package name and source file name
            self.report
                .source_files
                .insert(source_file_name.clone(), uuid_to_insert);
            self.report.number_of.source_files += 1;
            self.active_source_files.insert(uuid_to_insert, true);
            uuid_to_insert
        } else {
            self.get_source_file_uuid(source_file_name)
        }
    }
    pub fn register_texture(&mut self, name: String, file_path: &RelativePath, bytes: Vec<u8>) {
        assert!(
            self.textures.insert(file_path.clone(), bytes).is_none(),
            "duplicate image file {name}"
        );
        self.report.number_of.textures += 1;
    }

    pub fn register_uuid(
        &mut self,
        full_category_name: &String,
        uuid: &Uuid,
    ) -> Result<Uuid, String> {
        if let Some(parent_uuid) = self.all_categories.get(full_category_name) {
            let mut uuid_to_insert = *uuid;
            while self.entities_parents.contains_key(&uuid_to_insert) {
                trace!(
                    "Uuid collision detected {} for elements in {}",
                    uuid_to_insert,
                    full_category_name
                );
                uuid_to_insert = Uuid::new_v4();
            }
            self.entities_parents.insert(uuid_to_insert, *parent_uuid);
            self.report.number_of.entities += 1;
            Ok(uuid_to_insert)
        } else {
            // Dirty package ! We could fix it by making usage of the relative category the node is in.
            Err(format!(
                "Can't register world entity {} {}, no associated category found.",
                full_category_name, uuid
            ))
        }
    }

    pub fn register_marker(
        &mut self,
        full_category_name: String,
        mut marker: Marker,
    ) -> Result<(), String> {
        let uuid_to_insert = self.register_uuid(&full_category_name, &marker.guid)?;
        marker.guid = uuid_to_insert;
        if let std::collections::hash_map::Entry::Vacant(e) = self.maps.entry(marker.map_id) {
            e.insert(MapData::default());
            self.report.number_of.maps += 1;
        }
        self.maps
            .get_mut(&marker.map_id)
            .unwrap()
            .markers
            .insert(uuid_to_insert, marker);
        self.report.number_of.markers += 1;
        Ok(())
    }

    pub fn register_trail(
        &mut self,
        full_category_name: String,
        mut trail: Trail,
    ) -> Result<(), String> {
        let uuid_to_insert = self.register_uuid(&full_category_name, &trail.guid)?;
        trail.guid = uuid_to_insert;
        if let std::collections::hash_map::Entry::Vacant(e) = self.maps.entry(trail.map_id) {
            e.insert(MapData::default());
            self.report.number_of.maps += 1;
        }
        self.maps
            .get_mut(&trail.map_id)
            .unwrap()
            .trails
            .insert(uuid_to_insert, trail);
        self.report.number_of.trails += 1;
        Ok(())
    }

    pub fn register_route(&mut self, mut route: Route) -> Result<(), String> {
        let file_name = format!("data/dynamic_trails/{}.trl", &route.guid);
        let tbin_path: RelativePath = file_name.parse().unwrap();
        let uuid_to_insert = self.register_uuid(&route.category, &route.guid)?;
        route.guid = uuid_to_insert;
        let trail = route_to_trail(&route, &tbin_path);
        let tbin = route_to_tbin(&route);

        self.tbins.insert(tbin_path, tbin); //there may be duplicates since we load and save each time
        if let std::collections::hash_map::Entry::Vacant(e) = self.maps.entry(trail.map_id) {
            e.insert(MapData::default());
            self.report.number_of.maps += 1;
        }
        self.maps
            .get_mut(&trail.map_id)
            .unwrap()
            .trails
            .insert(uuid_to_insert, trail);
        self.maps
            .get_mut(&route.map_id)
            .unwrap()
            .routes
            .insert(uuid_to_insert, route);
        self.report.number_of.routes += 1;
        Ok(())
    }

    pub fn register_categories(&mut self) {
        let mut entities_parents: HashMap<Uuid, Uuid> = Default::default();
        let mut all_categories: HashMap<String, Uuid> = Default::default();
        Self::recursive_register_categories(
            &mut entities_parents,
            &self.categories,
            &mut all_categories,
        );
        self.entities_parents.extend(entities_parents);
        self.report.number_of.categories = all_categories.len();
        self.all_categories = all_categories;
    }
    fn recursive_register_categories(
        entities_parents: &mut HashMap<Uuid, Uuid>,
        categories: &OrderedHashMap<Uuid, Category>,
        all_categories: &mut HashMap<String, Uuid>,
    ) {
        for (_, cat) in categories.iter() {
            debug!(
                "Register category {} {} {:?}",
                cat.full_category_name, cat.guid, cat.parent
            );
            all_categories.insert(cat.full_category_name.clone(), cat.guid);
            if let Some(parent) = cat.parent {
                entities_parents.insert(cat.guid, parent);
            }
            Self::recursive_register_categories(entities_parents, &cat.children, all_categories);
        }
    }

    pub fn found_missing_element_texture(
        &mut self,
        file_path: String,
        requester_uuid: Uuid,
        source_file_uuid: &Uuid,
    ) {
        self.report.found_missing_texture(&file_path);
        let source_file_name = self
            .report
            .source_file_uuid_to_name(source_file_uuid)
            .unwrap();
        self.report.missing_textures.push(PackageElementSource {
            file_path,
            requester_reference: ElementReference::Uuid(requester_uuid),
            source_file_name: source_file_name.clone(),
        });
    }
    pub fn found_missing_inherited_texture(
        &mut self,
        file_path: String,
        full_category_name: String,
        source_file_uuid: &Uuid,
    ) {
        self.report.found_missing_texture(&file_path);
        let source_file_name = self
            .report
            .source_file_uuid_to_name(source_file_uuid)
            .unwrap();
        self.report.missing_textures.push(PackageElementSource {
            file_path,
            requester_reference: ElementReference::Category(full_category_name),
            source_file_name: source_file_name.clone(),
        });
    }
    pub fn found_missing_trail(
        &mut self,
        file_path: &RelativePath,
        requester_uuid: Uuid,
        source_file_uuid: &Uuid,
    ) {
        let source_file_name = self
            .report
            .source_file_uuid_to_name(source_file_uuid)
            .unwrap();
        self.report.missing_trails.push(PackageElementSource {
            file_path: file_path.as_str().to_string(),
            requester_reference: ElementReference::Uuid(requester_uuid),
            source_file_name: source_file_name.clone(),
        });
    }
}
