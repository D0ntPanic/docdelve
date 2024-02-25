use crate::chest::Chest;
use crate::db::{SearchParameters, Theme};
use anyhow::Result;
use btree_range_map::RangeMap;
use code_fuzzy_match::FuzzyMatcher;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt::Display;
use std::ops::Range;
use uuid::Uuid;

/// Minimum score to return a match. This requires at least two characters in the query or
/// a match of a one character query at the start of a word.
const MIN_SEARCH_SCORE: usize = 9;

/// Information about a chest.
#[derive(Serialize, Deserialize)]
pub struct ChestInfo {
    pub name: String,
    pub identifier: String,
    pub category_tag: String,
    pub extension_module: Option<String>,
    pub version: String,
    pub start_url: String,
    pub light_mode: Option<ThemeAdjustment>,
    pub dark_mode: Option<ThemeAdjustment>,
}

/// List of items contained in a chest along with the information about the chest.
#[derive(Serialize, Deserialize)]
pub struct ChestContents {
    #[serde(flatten)]
    pub info: ChestInfo,
    pub items: Vec<ChestItem>,
}

/// Chest contents optimized for searching.
pub struct IndexedChestContents {
    pub info: ChestInfo,
    items: Vec<IndexedChestItem>,
    root_item_ids: Vec<IndexedChestItemId>,
}

/// Reference to an item in [IndexedChestContents].
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct IndexedChestItemId(usize);

/// List of adjustments to apply for a given theme.
#[derive(Serialize, Deserialize, Default)]
pub struct ThemeAdjustment {
    pub file_replacements: Vec<FileReplacementRule>,
}

/// Rule for replacing file paths when reading from the chest. The `pattern` will be
/// matched as whole path elements at the end of the path, unless it starts with a '/'.
/// The `replacement` will be used to replace the matched path elements, or replaces the
/// entire path if it starts with a '/'.
#[derive(Serialize, Deserialize)]
pub struct FileReplacementRule {
    pub pattern: String,
    pub replacement: String,
}

/// A single item contained in a chest. May contain other items.
#[derive(Serialize, Deserialize)]
pub enum ChestItem {
    Module(Box<Module>),
    Group(Box<Group>),
    Page(Box<Page>),
    Object(Box<Object>),
}

/// An item contained in a chest in indexed form, with the path to the item.
pub struct IndexedChestItem {
    parent_path: Vec<IndexedChestItemId>,
    children: Range<usize>,
    pub data: IndexedChestItemData,
}

/// A single item contained in a chest in indexed form. Any contained items will be
/// referenced using an [IndexedChestItemId].
pub enum IndexedChestItemData {
    Module(IndexedModule),
    Group(IndexedGroup),
    Page(Page),
    Object(IndexedObject),
}

/// Information about a module in a chest.
#[derive(Serialize, Deserialize)]
pub struct ModuleInfo {
    pub name: String,
    pub full_name: String,
    pub url: Option<String>,
}

/// A module contained within a chest. A module is a collection of items within a namespace.
#[derive(Serialize, Deserialize)]
pub struct Module {
    #[serde(flatten)]
    pub info: ModuleInfo,
    pub contents: Vec<ChestItem>,
}

/// A module contained within a chest in indexed form. Any contained items will be referenced
/// using an [IndexedChestItemId].
pub struct IndexedModule {
    pub info: ModuleInfo,
    contents: Vec<IndexedChestItemId>,
}

/// Information about a group in a chest.
#[derive(Serialize, Deserialize)]
pub struct GroupInfo {
    pub name: String,
    pub url: Option<String>,
}

/// A named group of items contained within a chest.
#[derive(Serialize, Deserialize)]
pub struct Group {
    #[serde(flatten)]
    pub info: GroupInfo,
    pub contents: Vec<ChestItem>,
}

/// A named group of items contained within a chest in indexed form. Any contained items will be
/// referenced using an [IndexedChestItemId].
pub struct IndexedGroup {
    pub info: GroupInfo,
    contents: Vec<IndexedChestItemId>,
}

/// A text page contained within a chest. Also contains a table of contents.
#[derive(Serialize, Deserialize)]
pub struct Page {
    pub title: String,
    pub url: String,
    pub contents: Vec<PageItem>,
}

/// A table of contents item for a page. May contain other items.
#[derive(Serialize, Deserialize)]
pub enum PageItem {
    Category(Box<PageCategory>),
    Link(PageLink),
}

/// A category within the table of contents for a page. Can contain links or other categories.
#[derive(Serialize, Deserialize)]
pub struct PageCategory {
    pub title: String,
    pub url: Option<String>,
    pub contents: Vec<PageItem>,
}

/// A single link for the table of contents.
#[derive(Serialize, Deserialize, Clone)]
pub struct PageLink {
    pub title: String,
    pub url: String,
}

/// Information about an object in a chest.
#[derive(Serialize, Deserialize)]
pub struct ObjectInfo {
    pub name: String,
    pub full_name: String,
    pub declaration: Option<String>,
    pub url: Option<String>,
    pub object_type: ObjectType,
    pub bases: Vec<ChestPath>,
}

/// A programming language object contained within a chest. May contain other objects.
#[derive(Serialize, Deserialize)]
pub struct Object {
    #[serde(flatten)]
    pub info: ObjectInfo,
    pub contents: Vec<ChestItem>,
}

/// A programming language object contained within a chest in indexed form. Any contained items
/// will be referenced using an [IndexedChestItemId].
pub struct IndexedObject {
    pub info: ObjectInfo,
    contents: Vec<IndexedChestItemId>,
}

/// Type of programming language object.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ObjectType {
    Class,
    Struct,
    Union,
    Object,
    Enum,
    Value,
    Variant,
    Trait,
    TraitImplementation,
    Interface,
    Function,
    Method,
    Variable,
    Member,
    Field,
    Constant,
    Property,
    Typedef,
    Namespace,
}

/// Type of element in a chest path.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum ChestPathElementType {
    Module,
    Group,
    Page,
    Object,
}

/// A single element in a chest path.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ChestPathElement {
    pub element_type: ChestPathElementType,
    pub name: String,
}

/// A single element in a chest path.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ChestPathElementRef<'a> {
    pub element_type: ChestPathElementType,
    pub name: &'a str,
}

/// Path to an item in a chest.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Debug)]
pub struct ChestPath {
    pub elements: Vec<ChestPathElement>,
}

/// A single search result within a chest.
#[derive(Clone, PartialEq, Eq)]
pub struct ChestSearchResult {
    pub path: ChestPath,
    pub score: usize,
}

/// A single search result within a chest in indexed form.
#[derive(Clone, PartialEq, Eq)]
struct IndexedChestSearchResult {
    pub item: IndexedChestItemId,
    pub score: usize,
}

impl ChestContents {
    /// Create a new empty chest.
    pub fn new(
        name: &str,
        category_tag: &str,
        extension_module: Option<&str>,
        version: &str,
        start_url: &str,
        light_mode: Option<ThemeAdjustment>,
        dark_mode: Option<ThemeAdjustment>,
    ) -> Self {
        Self {
            info: ChestInfo {
                name: name.to_string(),
                identifier: Uuid::new_v4().simple().to_string(),
                category_tag: category_tag.to_string(),
                extension_module: extension_module.map(|s| s.to_string()),
                version: version.to_string(),
                start_url: start_url.to_string(),
                light_mode,
                dark_mode,
            },
            items: Vec::new(),
        }
    }

    /// Read the contents of a chest from a chest.
    pub fn read_from_chest(chest: &Chest) -> Result<Self> {
        let contents = String::from_utf8(chest.read("_chest_contents.json")?)?;
        let result: ChestContents = serde_json::from_str(&contents)?;
        Ok(result)
    }

    /// Writes the chest contents to a chest.
    pub fn write_to_chest(&self, chest: &mut Chest) -> Result<()> {
        let contents = serde_json::to_string(self)?;
        chest.write("_chest_contents.json", contents.as_bytes())?;
        Ok(())
    }

    /// Converts chest contents into indexed form.
    pub fn to_indexed(self) -> IndexedChestContents {
        let mut items = Vec::new();
        let mut path = Vec::new();
        let root_item_ids = Self::indexed_contents(self.items, &mut items, &mut path);
        IndexedChestContents {
            info: self.info,
            items,
            root_item_ids,
        }
    }

    /// Converts a list of chest items into indexed form.
    fn indexed_contents(
        contents: Vec<ChestItem>,
        items: &mut Vec<IndexedChestItem>,
        path: &mut Vec<IndexedChestItemId>,
    ) -> Vec<IndexedChestItemId> {
        let mut result = Vec::new();
        for item in contents {
            let id = IndexedChestItemId(items.len());
            let parent_path = path.clone();
            result.push(id);

            let (data, contents) = match item {
                ChestItem::Module(module) => (
                    IndexedChestItemData::Module(IndexedModule {
                        info: module.info,
                        contents: Vec::new(),
                    }),
                    module.contents,
                ),
                ChestItem::Group(group) => (
                    IndexedChestItemData::Group(IndexedGroup {
                        info: group.info,
                        contents: Vec::new(),
                    }),
                    group.contents,
                ),
                ChestItem::Page(page) => (IndexedChestItemData::Page(*page), Vec::new()),
                ChestItem::Object(object) => (
                    IndexedChestItemData::Object(IndexedObject {
                        info: object.info,
                        contents: Vec::new(),
                    }),
                    object.contents,
                ),
            };
            items.push(IndexedChestItem {
                parent_path,
                data,
                children: id.0..id.0,
            });

            path.push(id);
            let first_child = IndexedChestItemId(items.len());
            let contents = Self::indexed_contents(contents, items, path);
            let end_of_children = IndexedChestItemId(items.len());
            if let Some(item) = items.get_mut(id.0) {
                item.children = first_child.0..end_of_children.0;
                match &mut item.data {
                    IndexedChestItemData::Module(module) => {
                        module.contents = contents;
                    }
                    IndexedChestItemData::Group(group) => {
                        group.contents = contents;
                    }
                    IndexedChestItemData::Page(_) => (),
                    IndexedChestItemData::Object(object) => {
                        object.contents = contents;
                    }
                }
            }
            path.pop();
        }
        result
    }
}

impl IndexedChestContents {
    /// Gets chest items by path.
    pub fn get(&self, path: &ChestPath) -> Vec<&IndexedChestItem> {
        self.get_ids(path)
            .into_iter()
            .filter_map(|id| self.get_by_id(id))
            .collect()
    }

    /// Gets the list of items at the root of the chest.
    pub fn items(&self) -> Vec<&IndexedChestItem> {
        self.root_item_ids
            .iter()
            .filter_map(|id| self.get_by_id(*id))
            .collect()
    }

    /// Gets chest item identifiers by path.
    fn get_ids(&self, path: &ChestPath) -> Vec<IndexedChestItemId> {
        let mut contents = self.root_item_ids.clone();
        let mut matching = Vec::new();
        for element in &path.elements {
            let mut next_contents = Vec::new();
            let mut next_matching = Vec::new();
            for item_id in &contents {
                if let Some(item) = self.get_by_id(*item_id) {
                    if &item.as_path_element() == element {
                        next_matching.push(*item_id);
                        next_contents.extend_from_slice(item.content_ids());
                    }
                }
            }
            contents = next_contents;
            matching = next_matching;
        }
        matching
    }

    /// Gets a chest item by identifier.
    fn get_by_id(&self, path: IndexedChestItemId) -> Option<&IndexedChestItem> {
        self.items.get(path.0)
    }

    /// Gets the path for an item by identifier.
    fn path_for_id(&self, id: IndexedChestItemId) -> Option<ChestPath> {
        self.get_by_id(id).map(|item| {
            let mut elements = Vec::new();
            elements.reserve(item.parent_path.len() + 1);
            for element_id in &item.parent_path {
                if let Some(element) = self.get_by_id(*element_id) {
                    elements.push(element.as_path_element());
                }
            }
            elements.push(item.as_path_element());
            ChestPath { elements }
        })
    }

    /// Searches a chest for items that match a string query. Search is performed within
    /// the given `start` path, or the entire chest if equal to [ChestPath::root]. The result
    /// is sorted by relevance, with the most relevant items first. Empty queries are not
    /// supported and return an empty result.
    pub fn search(
        &self,
        start: &ChestPath,
        query: &str,
        parameters: &SearchParameters,
    ) -> Vec<ChestSearchResult> {
        // Split the query into sub-queries separated by common programming language
        // separators like '.' and ':'. Empty sub-queries are removed so that constructs
        // like "::" are treated as a single separator.
        let mut parts = query.split(&['.', ':']).collect::<Vec<_>>();
        parts.retain(|part| !part.is_empty());

        // Split query into the last part, which will generate final results, and the parts
        // leading up to it, which will generate interval trees to chain the results.
        let (last_part, parts) = if let Some((last_part, parts)) = parts.split_last() {
            (last_part, parts)
        } else {
            // Empty query. There is nothing to base the results upon so abort now.
            return Vec::new();
        };

        // Initialize search space. If there was a requested start path, add the items
        // at that path as the initial search space.
        let mut search_space = RangeMap::new();
        if start.elements.len() > 0 {
            for item_id in self.get_ids(&start) {
                if let Some(item) = self.get_by_id(item_id) {
                    search_space.insert(item.children.clone(), 0);
                }
            }
        } else {
            search_space.insert(0..self.items.len(), 0);
        }

        // Perform each sub-query in sequence, narrowing the search space and collecting
        // the aggregate score for each part.
        let mut fuzzy_matcher = FuzzyMatcher::new();
        for part in parts {
            let mut new_search_space = RangeMap::new();
            self.search_items(
                &mut fuzzy_matcher,
                search_space,
                part,
                |item_id, item, score| {
                    // If the existing score for this item is already at least as good as the
                    // new score, we don't want to update the range with a worse score.
                    if let Some(existing_score) = new_search_space.get(item_id.0) {
                        if *existing_score >= score {
                            return;
                        }
                    }

                    new_search_space.insert(item.children.clone(), score);
                },
            );
            search_space = new_search_space;
        }

        // Perform the last part of the query and gather results.
        let mut results = Vec::new();
        self.search_items(
            &mut fuzzy_matcher,
            search_space,
            last_part,
            |item_id, _item, score| {
                results.push(IndexedChestSearchResult {
                    item: item_id,
                    score,
                });
            },
        );

        // Finalize results by sorting and truncating to the requested count
        results.par_sort_unstable_by(|a, b| self.compare_search_results(a, b));
        results.dedup_by(|a, b| self.compare_search_results(a, b) == Ordering::Equal);
        results.truncate(parameters.result_count);

        // Convert results into path form
        results
            .into_iter()
            .filter_map(|result| {
                self.path_for_id(result.item).map(|path| ChestSearchResult {
                    path,
                    score: result.score,
                })
            })
            .collect()
    }

    /// Searches a given set of items for items that match a string query. If results are
    /// found `func` is called for each result.
    fn search_items<F>(
        &self,
        fuzzy_matcher: &mut FuzzyMatcher,
        search_space: RangeMap<usize, usize>,
        query: &str,
        mut func: F,
    ) where
        F: FnMut(IndexedChestItemId, &IndexedChestItem, usize),
    {
        // Iterate over all ranges in the search space
        for (range, prior_score) in search_space.iter() {
            // Grab the first and last items for this range
            if let Some(first) = range.first() {
                if let Some(last) = range.last() {
                    // Iterate over items in the range
                    for item_id in first..=last {
                        let item_id = IndexedChestItemId(item_id);
                        if let Some(item) = self.get_by_id(item_id) {
                            // Check item for a match
                            if let Some(score) = fuzzy_matcher.fuzzy_match(item.name(), query) {
                                // Ensure score meets the minimum score requirement
                                if score >= MIN_SEARCH_SCORE {
                                    // Result found, score is the sum of the prior score from
                                    // the search space and this item's score.
                                    func(item_id, item, *prior_score + score);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Performs per-theme path transformation according to the chest configuration.
    pub fn transform_path_for_theme<'a>(&'a self, path: &'a str, theme: Theme) -> Cow<str> {
        if let Some(adjustment) = match theme {
            Theme::Light => &self.info.light_mode,
            Theme::Dark => &self.info.dark_mode,
        } {
            for replacement in &adjustment.file_replacements {
                if let Some(transform) =
                    Chest::transform_path(path, &replacement.pattern, &replacement.replacement)
                {
                    return Cow::Owned(transform);
                }
            }
            Cow::Borrowed(path)
        } else {
            Cow::Borrowed(path)
        }
    }

    /// Gets the item corresponding to the page that an item is present on.
    pub fn page_for_path(&self, url: &str, hint_path: Option<&ChestPath>) -> Option<ChestPath> {
        // Strip off any anchors from the URL
        let url = if let Some((base, _)) = url.rsplit_once('#') {
            base
        } else {
            url
        };

        // Strip off any leading slashes
        let url = if let Some(url_no_slash) = url.strip_prefix("/") {
            url_no_slash
        } else {
            url
        };

        // Get the list of possible paths to this chest
        let mut possible_items = Vec::new();
        self.search_for_url(&mut possible_items, &self.root_item_ids, url);
        possible_items.sort_by(|a, b| self.compare_item_paths(*b, *a));

        // If there is a path hint, check to see if one of the possible paths is a parent of
        // the given path hint. This iteration is performed in reverse sorted order so that
        // the deepest path will be checked first.
        if let Some(hint_path) = hint_path {
            for item_id in &possible_items {
                if let Some(path) = self.path_for_id(*item_id) {
                    if path.is_parent_of(&hint_path) {
                        return Some(path);
                    }
                }
            }
        }

        // Return first path in reverse sorted order as result, this will be the deepest match
        possible_items
            .first()
            .map(|item_id| self.path_for_id(*item_id))
            .flatten()
    }

    /// Recursively search a set of items for a given URL.
    fn search_for_url(
        &self,
        results: &mut Vec<IndexedChestItemId>,
        items: &[IndexedChestItemId],
        url: &str,
    ) {
        for item_id in items.iter() {
            if let Some(item) = self.get_by_id(*item_id) {
                if item.url() == Some(url) {
                    results.push(*item_id);
                }
                self.search_for_url(results, item.content_ids(), url);
            }
        }
    }

    /// Compares two search results for relevance.
    fn compare_search_results(
        &self,
        a: &IndexedChestSearchResult,
        b: &IndexedChestSearchResult,
    ) -> Ordering {
        a.score
            .cmp(&b.score)
            .reverse()
            .then_with(|| self.compare_item_paths(a.item, b.item))
    }

    /// Compares two item paths. Shorter paths come first.
    fn compare_item_paths(&self, a: IndexedChestItemId, b: IndexedChestItemId) -> Ordering {
        let a_item = self.get_by_id(a);
        let b_item = self.get_by_id(b);
        const EMPTY: &'static [IndexedChestItemId] = &[];
        let mut a_iter = a_item
            .map(|item| item.parent_path.iter())
            .unwrap_or(EMPTY.iter());
        let mut b_iter = b_item
            .map(|item| item.parent_path.iter())
            .unwrap_or(EMPTY.iter());
        loop {
            let a_element_id = a_iter.next();
            let b_element_id = b_iter.next();
            if a_element_id.is_none() && b_element_id.is_none() {
                return a_item
                    .map(|item| item.as_path_element_ref())
                    .cmp(&b_item.map(|item| item.as_path_element_ref()));
            } else if a_element_id.is_none() {
                return Ordering::Less;
            } else if b_element_id.is_none() {
                return Ordering::Greater;
            } else {
                let a_element = self
                    .get_by_id(*a_element_id.unwrap())
                    .map(|item| item.as_path_element_ref());
                let b_element = self
                    .get_by_id(*b_element_id.unwrap())
                    .map(|item| item.as_path_element_ref());
                let compare = a_element.cmp(&b_element);
                if compare != Ordering::Equal {
                    return compare;
                }
            }
        }
    }
}

impl ChestItem {
    /// Name of the chest item.
    pub fn name(&self) -> &str {
        match self {
            ChestItem::Module(module) => &module.info.name,
            ChestItem::Group(group) => &group.info.name,
            ChestItem::Page(page) => &page.title,
            ChestItem::Object(object) => &object.info.name,
        }
    }

    /// List of item identifiers contained in a chest item.
    pub fn contents(&self) -> &Vec<ChestItem> {
        const EMPTY: &'static Vec<ChestItem> = &vec![];
        match self {
            ChestItem::Module(module) => &module.contents,
            ChestItem::Group(group) => &group.contents,
            ChestItem::Page(_) => EMPTY,
            ChestItem::Object(object) => &object.contents,
        }
    }

    /// Mutable list of items contained in a chest item.
    pub fn contents_mut(&mut self) -> Option<&mut Vec<ChestItem>> {
        match self {
            ChestItem::Module(module) => Some(&mut module.contents),
            ChestItem::Group(group) => Some(&mut group.contents),
            ChestItem::Page(_) => None,
            ChestItem::Object(object) => Some(&mut object.contents),
        }
    }

    /// Type of the chest item.
    pub fn element_type(&self) -> ChestPathElementType {
        match self {
            ChestItem::Module(_) => ChestPathElementType::Module,
            ChestItem::Group(_) => ChestPathElementType::Group,
            ChestItem::Page(_) => ChestPathElementType::Page,
            ChestItem::Object(_) => ChestPathElementType::Object,
        }
    }

    /// Gets the path element that should be used to reference this chest item
    pub fn as_path_element(&self) -> ChestPathElement {
        ChestPathElement {
            element_type: self.element_type(),
            name: self.name().to_string(),
        }
    }

    /// Checks to see if this chest item matches the given path element.
    pub fn matches_path_element(&self, element: &ChestPathElement) -> bool {
        element.element_type == self.element_type() && element.name == self.name()
    }
}

impl IndexedChestItem {
    /// Name of the chest item.
    pub fn name(&self) -> &str {
        match &self.data {
            IndexedChestItemData::Module(module) => &module.info.name,
            IndexedChestItemData::Group(group) => &group.info.name,
            IndexedChestItemData::Page(page) => &page.title,
            IndexedChestItemData::Object(object) => &object.info.name,
        }
    }

    /// URL of the chest item.
    pub fn url(&self) -> Option<&str> {
        match &self.data {
            IndexedChestItemData::Module(module) => module.info.url.as_ref().map(|s| s.as_str()),
            IndexedChestItemData::Group(group) => group.info.url.as_ref().map(|s| s.as_str()),
            IndexedChestItemData::Page(page) => Some(&page.url),
            IndexedChestItemData::Object(object) => object.info.url.as_ref().map(|s| s.as_str()),
        }
    }

    /// List of items contained in a chest item.
    pub fn contents<'a>(&self, chest: &'a IndexedChestContents) -> Vec<&'a IndexedChestItem> {
        self.content_ids()
            .iter()
            .filter_map(|id| chest.get_by_id(*id))
            .collect()
    }

    /// List of item identifiers contained in a chest item.
    fn content_ids(&self) -> &[IndexedChestItemId] {
        const EMPTY: &'static [IndexedChestItemId] = &[];
        match &self.data {
            IndexedChestItemData::Module(module) => &module.contents,
            IndexedChestItemData::Group(group) => &group.contents,
            IndexedChestItemData::Page(_) => EMPTY,
            IndexedChestItemData::Object(object) => &object.contents,
        }
    }

    /// Type of the chest item.
    pub fn element_type(&self) -> ChestPathElementType {
        match &self.data {
            IndexedChestItemData::Module(_) => ChestPathElementType::Module,
            IndexedChestItemData::Group(_) => ChestPathElementType::Group,
            IndexedChestItemData::Page(_) => ChestPathElementType::Page,
            IndexedChestItemData::Object(_) => ChestPathElementType::Object,
        }
    }

    /// Gets the path element that should be used to reference this chest item
    pub fn as_path_element(&self) -> ChestPathElement {
        ChestPathElement {
            element_type: self.element_type(),
            name: self.name().to_string(),
        }
    }

    /// Gets the path element that should be used to reference this chest item
    pub fn as_path_element_ref(&self) -> ChestPathElementRef {
        ChestPathElementRef {
            element_type: self.element_type(),
            name: self.name(),
        }
    }

    /// Checks to see if this chest item matches the given path element.
    pub fn matches_path_element(&self, element: &ChestPathElement) -> bool {
        element.element_type == self.element_type() && element.name == self.name()
    }
}

impl ChestPathElement {
    /// Converts a list of chest items to a list of path elements. Names that
    /// have more than one item are collapsed into a single path element.
    pub fn path_elements_for_items(items: &[IndexedChestItem]) -> Vec<ChestPathElement> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for item in items {
            let element = item.as_path_element();
            if !seen.contains(&element) {
                seen.insert(element.clone());
                result.push(element);
            }
        }
        result
    }
}

impl PartialOrd for ChestPathElement {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.name
                .cmp(&other.name)
                .then_with(|| self.element_type.cmp(&other.element_type)),
        )
    }
}

impl Ord for ChestPathElement {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl<'a> PartialOrd for ChestPathElementRef<'a> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.name
                .cmp(&other.name)
                .then_with(|| self.element_type.cmp(&other.element_type)),
        )
    }
}

impl<'a> Ord for ChestPathElementRef<'a> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl ChestPath {
    /// Path to the root of the chest.
    pub fn root() -> Self {
        ChestPath {
            elements: Vec::new(),
        }
    }

    /// Returns `true` if this path is a parent of `other`.
    pub fn is_parent_of(&self, other: &Self) -> bool {
        if self.elements.len() > other.elements.len() {
            return false;
        }
        self.elements
            .iter()
            .zip(other.elements.iter())
            .all(|(a, b)| a == b)
    }
}

impl PartialOrd for ChestPath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.elements
                .len()
                .cmp(&other.elements.len())
                .then_with(|| self.elements.cmp(&other.elements)),
        )
    }
}

impl Ord for ChestPath {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Display for ChestPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.elements.is_empty() {
            write!(f, "/")
        } else {
            for element in &self.elements {
                write!(f, "/{}", element.name)?;
            }
            Ok(())
        }
    }
}
