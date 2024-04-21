use crate::{attributes::CommonAttributes, package::PackageImportReport};
use ordered_hash_map::OrderedHashMap;
use tracing::debug;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RawCategory {
    pub guid: Uuid,
    pub parent_name: Option<String>,
    pub display_name: String,
    pub relative_category_name: String,
    pub full_category_name: String,
    pub separator: bool,
    pub default_enabled: bool,
    pub props: CommonAttributes,
    pub sources: OrderedHashMap<Uuid, Uuid>,
}

#[derive(Debug, Clone)]
pub struct Category {
    pub guid: Uuid,
    pub parent: Option<Uuid>,
    pub display_name: String,
    pub relative_category_name: String,
    pub full_category_name: String,
    pub separator: bool,
    pub default_enabled: bool,
    pub props: CommonAttributes,
    pub children: OrderedHashMap<Uuid, Category>, //TODO: make a branch to test if having an Vec<Uuid> associated with global list of categories is faster.
}

pub fn nth_chunk(s: &str, pat: char, n: usize) -> String {
    let nb_matches = s.matches(pat).count();
    assert!(nb_matches + 1 > n);
    let res = s.split(pat).nth(n);
    debug!("nth_chunk {} {} {:?}", s, n, res);
    res.unwrap().to_string()
}

pub fn prefix_until_nth_char(s: &str, pat: char, n: usize) -> Option<String> {
    let res = s
        .match_indices(pat)
        .nth(n)
        .map(|(index, _)| s.split_at(index))
        .map(|(left, _)| left.to_string());
    debug!("prefix_until_nth_char {} {} {:?}", s, n, res);
    res
}

pub fn prefix_parent(s: &str, pat: char) -> Option<String> {
    let n = s.matches(pat).count();
    assert!(n > 0);
    let res = s
        .match_indices(pat)
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
            guid: value.guid,
            props: value.props.clone(),
            separator: value.separator,
            default_enabled: value.default_enabled,
            display_name: value.display_name.clone(),
            relative_category_name: value.relative_category_name.clone(),
            full_category_name: value.full_category_name.clone(),
            parent,
            children: Default::default(),
        }
    }
    fn per_route<'a>(
        categories: &'a mut OrderedHashMap<Uuid, Category>,
        route: &[&str],
    ) -> Option<&'a mut Category> {
        let mut route = route.to_owned();
        route.reverse();
        Category::_per_route(categories, &mut route)
    }
    fn _per_route<'a>(
        categories: &'a mut OrderedHashMap<Uuid, Category>,
        route: &mut Vec<&str>,
    ) -> Option<&'a mut Category> {
        if let Some(relative_category_name) = route.pop() {
            for (_, cat) in categories {
                if cat.relative_category_name == relative_category_name {
                    if route.is_empty() {
                        return Some(cat);
                    } else {
                        return Category::_per_route(&mut cat.children, route);
                    }
                }
            }
        }
        None
    }
    #[allow(dead_code)]
    fn per_uuid<'a>(
        categories: &'a mut OrderedHashMap<Uuid, Category>,
        uuid: &Uuid,
    ) -> Option<&'a mut Category> {
        /*
        Do a look up in the tree based on uuid. Whole tree is scanned until a match is found.

        WARNING: very inefficient in the general case.
        */
        for (_, cat) in categories {
            if &cat.guid == uuid {
                return Some(cat);
            }
            let sub_res = Category::per_uuid(&mut cat.children, uuid);
            if sub_res.is_some() {
                return sub_res;
            }
        }
        None
    }
    pub fn reassemble(
        input_first_pass_categories: &OrderedHashMap<String, RawCategory>,
        report: &mut PackageImportReport,
    ) -> OrderedHashMap<Uuid, Category> {
        let start_initialize = std::time::SystemTime::now();
        let mut first_pass_categories = input_first_pass_categories.clone();
        let mut second_pass_categories: OrderedHashMap<String, RawCategory> = Default::default();
        let mut need_a_pass: bool = true;

        let mut third_pass_categories: OrderedHashMap<Uuid, Category> = Default::default();
        let mut third_pass_categories_ref: Vec<Uuid> = Default::default();
        let mut root: OrderedHashMap<Uuid, Category> = Default::default();

        let elaspsed_initialize = start_initialize.elapsed().unwrap_or_default();
        report.telemetry.categories_reassemble.initialize = elaspsed_initialize.as_millis();

        let start_multi_pass_missing_categories_creation = std::time::SystemTime::now();
        let mut nb_pass_done = 0;
        while need_a_pass {
            need_a_pass = false;
            nb_pass_done += 1;
            for (key, value) in first_pass_categories.iter() {
                debug!("reassemble_categories pass #{} {:?}", nb_pass_done, value);
                let mut to_insert = value.clone();
                if value.relative_category_name.matches('.').count() > 0
                    && value.relative_category_name == value.full_category_name
                {
                    let mut n = 0;
                    let mut last_name: Option<String> = None;
                    // This is an almost duplication of code of pack/mod.rs
                    while let Some(parent_name) =
                        prefix_until_nth_char(&value.relative_category_name, '.', n)
                    {
                        debug!("{} {}", parent_name, n);
                        if let Some(parent_category) = first_pass_categories.get(&parent_name) {
                            report.found_category_late(&parent_name, parent_category.guid);
                            last_name = Some(parent_name.clone());
                        } else if let Some(parent_category) =
                            second_pass_categories.get(&parent_name)
                        {
                            report.found_category_late(&parent_name, parent_category.guid);
                            last_name = Some(parent_name.clone());
                        } else {
                            let new_uuid = Uuid::new_v4();
                            let relative_category_name =
                                nth_chunk(&value.relative_category_name, '.', n);
                            debug!("reassemble_categories Partial create missing parent category: {} {} {} {}", parent_name, relative_category_name, n, new_uuid);
                            let sources: OrderedHashMap<Uuid, Uuid> = OrderedHashMap::new();
                            let to_insert = RawCategory {
                                default_enabled: value.default_enabled,
                                guid: new_uuid,
                                relative_category_name: relative_category_name.clone(),
                                display_name: relative_category_name.clone(),
                                parent_name: prefix_until_nth_char(&parent_name, '.', n - 1),
                                props: value.props.clone(),
                                separator: false,
                                full_category_name: parent_name.clone(),
                                sources,
                            };
                            last_name = Some(to_insert.full_category_name.clone());
                            report.found_category_late(&to_insert.full_category_name, new_uuid);
                            second_pass_categories.insert(parent_name.clone(), to_insert);
                            need_a_pass = true;
                        }
                        n += 1;
                    }
                    for (requester_uuid, source_file_uuid) in value.sources.iter() {
                        report.found_category_late_with_details(
                            &value.full_category_name,
                            value.guid,
                            requester_uuid,
                            source_file_uuid,
                        );
                    }
                    report.found_category_late(&value.full_category_name, value.guid);
                    to_insert.relative_category_name =
                        nth_chunk(&value.relative_category_name, '.', n);
                    to_insert.display_name = to_insert.relative_category_name.clone();
                    debug!(
                        "parent_name: {:?}, new name: {}, old name: {}",
                        last_name, to_insert.relative_category_name, &value.relative_category_name
                    );
                    assert!(last_name.is_some());
                    to_insert.parent_name = last_name;
                } else {
                    to_insert.parent_name = if let Some(parent_name) = &value.parent_name {
                        first_pass_categories
                            .get(parent_name)
                            .map(|parent_category| parent_category.full_category_name.clone())
                    } else {
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
        let elaspsed_multi_pass_missing_categories_creation =
            start_multi_pass_missing_categories_creation
                .elapsed()
                .unwrap_or_default();
        report
            .telemetry
            .categories_reassemble
            .missing_categories_creation =
            elaspsed_multi_pass_missing_categories_creation.as_millis();

        debug!("nb_pass_done {}", nb_pass_done);
        let start_parent_child_relationship = std::time::SystemTime::now();
        for (key, value) in second_pass_categories {
            let parent = if let Some(parent_name) = &value.parent_name {
                first_pass_categories
                    .get(parent_name)
                    .map(|parent_category| parent_category.guid)
            } else {
                None
            };

            debug!("{} parent is {:?}", key, parent);
            let cat = Category::from(&value, parent);
            let cat_ref = cat.guid;
            if third_pass_categories.insert(cat.guid, cat).is_none() {
                third_pass_categories_ref.push(cat_ref);
            }
        }
        let elaspsed_parent_child_relationship = start_parent_child_relationship
            .elapsed()
            .unwrap_or_default();
        report
            .telemetry
            .categories_reassemble
            .parent_child_relationship = elaspsed_parent_child_relationship.as_millis();

        debug!("third_pass_categories_ref");
        let start_tree_insertion = std::time::SystemTime::now();
        for full_category_uuid in third_pass_categories_ref {
            if let Some(cat) = third_pass_categories.remove(&full_category_uuid) {
                let mut route = Vec::from_iter(cat.full_category_name.split('.'));
                route.pop(); //it is now the parent route
                if let Some(parent) = cat.parent {
                    if let Some(parent_category) =
                        Category::per_route(&mut third_pass_categories, &route)
                    {
                        parent_category.children.insert(cat.guid, cat);
                    } else if let Some(parent_category) = Category::per_route(&mut root, &route) {
                        parent_category.children.insert(cat.guid, cat);
                    } else {
                        panic!("Could not find parent {} for {:?}", parent, cat);
                    }
                } else {
                    root.insert(cat.guid, cat);
                }
            } else {
                panic!("Some bad logic at works");
            }
        }
        let elaspsed_tree_insertion = start_tree_insertion.elapsed().unwrap_or_default();
        report.telemetry.categories_reassemble.tree_insertion = elaspsed_tree_insertion.as_millis();
        debug!("reassemble_categories end {:?}", root);
        root
    }
}
