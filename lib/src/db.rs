use crate::chest::{Chest, ChestListEntry};
use crate::content::{
    ChestContents, ChestPath, IndexedChestContents, IndexedChestItem, IndexedChestItemData,
    PageItem,
};
use anyhow::{anyhow, Result};
use directories::ProjectDirs;
use rayon::prelude::*;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Database of all available chests.
pub struct Database {
    data_path: PathBuf,
    identifiers: BTreeMap<String, LoadedChest>,
    tags: BTreeMap<String, TagVersions>,
}

/// A loaded chest with the files and semantic contents of the chest.
struct LoadedChest {
    chest: Chest,
    contents: IndexedChestContents,
}

/// Database of all available versions of a specific chest identifier.
#[derive(Default)]
struct TagVersions {
    latest_version: String,
    versions: BTreeMap<String, String>,
}

/// Path to an item within all chests.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ItemPath {
    pub identifier: String,
    pub chest_path: ChestPath,
}

/// Parameters for searching a chest.
#[derive(Clone)]
pub struct SearchParameters {
    pub result_count: usize,
}

/// A single search result.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct SearchResult {
    pub path: ItemPath,
    pub score: usize,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Theme {
    Dark,
    Light,
}

pub struct ItemContents<'a> {
    pub chest_items: Vec<&'a IndexedChestItem>,
    pub page_items: Vec<&'a PageItem>,
    pub bases: Vec<ChestPath>,
}

impl Database {
    /// Loads the database and chests from disk.
    pub fn load() -> Result<Self> {
        // Get the platform specific user directory where the chests are stored
        let project_dirs = ProjectDirs::from("", "", "docdelve")
            .ok_or_else(|| anyhow!("Invalid user directory"))?;
        let data_path = project_dirs.data_local_dir().join("chests");

        // Load all chests into the database
        let mut identifiers: BTreeMap<String, LoadedChest> = BTreeMap::new();
        let mut tags: BTreeMap<String, TagVersions> = BTreeMap::new();
        if data_path.exists() {
            for entry in data_path.read_dir()? {
                let entry = entry?;
                if entry.file_type()?.is_file()
                    && entry.file_name().to_string_lossy().ends_with(".ddchest")
                {
                    if let Ok(chest) = Chest::open(&data_path.join(entry.file_name())) {
                        if let Ok(contents) = ChestContents::read_from_chest(&chest) {
                            tags.entry(contents.info.category_tag.clone())
                                .or_default()
                                .versions
                                .insert(
                                    contents.info.version.clone(),
                                    contents.info.identifier.clone(),
                                );
                            let identifier = contents.info.identifier.clone();
                            identifiers.insert(
                                identifier,
                                LoadedChest {
                                    chest,
                                    contents: contents.to_indexed(),
                                },
                            );
                        }
                    }
                }
            }
        }

        // For each chest identifier, detect the latest version
        for (_, identifier_versions) in tags.iter_mut() {
            let mut versions = identifier_versions.versions.keys().collect::<Vec<_>>();
            versions.sort_by_key(|version| Self::semantic_version(version));
            if let Some(latest) = versions.last() {
                identifier_versions.latest_version = (*latest).clone();
            }
        }

        Ok(Self {
            data_path,
            identifiers,
            tags,
        })
    }

    /// Convert the version string into a semantic version that can be compared for
    /// detecting the latest version.
    fn semantic_version(version: &str) -> Vec<u32> {
        let mut result = Vec::new();
        version.split(&['.', '-', '_']).for_each(|part| {
            if let Ok(num) = part.parse::<u32>() {
                result.push(num);
            }
        });
        result
    }

    /// Installs a chest into the database.
    pub fn install(&mut self, chest: &Chest) -> Result<()> {
        // Load the chest contents, also ensures that it is a valid chest
        let contents = ChestContents::read_from_chest(&chest)?;

        // Copy the chest file into the data path
        let path = chest.path().ok_or_else(|| anyhow!("Chest has no path"))?;
        let target_path = self.data_path.join(
            path.file_name()
                .ok_or_else(|| anyhow!("Chest path has no filename"))?,
        );
        std::fs::create_dir_all(&self.data_path)?;
        std::fs::copy(&path, &target_path)?;

        // Reopen chest from new path. This frees up the original file so that it can be closed
        // and deleted if necessary.
        let chest = Chest::open(&target_path)?;

        // Insert the chest into the database
        let tag_versions = self
            .tags
            .entry(contents.info.category_tag.clone())
            .or_default();
        tag_versions.versions.insert(
            contents.info.version.clone(),
            contents.info.identifier.clone(),
        );

        let identifier = contents.info.identifier.clone();
        self.identifiers.insert(
            identifier,
            LoadedChest {
                chest,
                contents: contents.to_indexed(),
            },
        );

        // Reevaluate latest version for this identifier
        let mut versions = tag_versions.versions.values().collect::<Vec<_>>();
        versions.sort_by_key(|version| Self::semantic_version(version));
        if let Some(latest) = versions.last() {
            tag_versions.latest_version = (*latest).clone();
        }

        Ok(())
    }

    /// Gets a chest's contents by its identifier.
    pub fn chest(&self, identifier: &str) -> Option<&IndexedChestContents> {
        self.identifiers
            .get(identifier)
            .map(|chest| &chest.contents)
    }

    /// Gets chest item(s) by path.
    pub fn items_at_path(&self, path: &ItemPath) -> Vec<&IndexedChestItem> {
        if let Some(chest) = self.identifiers.get(&path.identifier) {
            return chest.contents.get(&path.chest_path);
        }
        Vec::new()
    }

    /// Gets chest item contents by path.
    pub fn item_contents_at_path(&self, path: &ItemPath) -> ItemContents {
        if let Some(chest) = self.identifiers.get(&path.identifier) {
            let items = chest.contents.get(&path.chest_path);
            if items.len() == 1 {
                // One item, return the contents directly
                match &items[0].data {
                    IndexedChestItemData::Page(page) => ItemContents {
                        chest_items: Vec::new(),
                        page_items: page.info.contents.iter().collect(),
                        bases: Vec::new(),
                    },
                    IndexedChestItemData::Object(object) => ItemContents {
                        chest_items: items[0].contents(&chest.contents),
                        page_items: Vec::new(),
                        bases: object.info.bases.clone(),
                    },
                    _ => ItemContents {
                        chest_items: items[0].contents(&chest.contents),
                        page_items: Vec::new(),
                        bases: Vec::new(),
                    },
                }
            } else {
                // More than one item at the given path, return the combination of
                // all contents.
                let mut chest_items = Vec::new();
                let mut page_items = Vec::new();
                let mut bases = Vec::new();
                for item in items {
                    match &item.data {
                        IndexedChestItemData::Page(page) => {
                            page_items.extend(page.info.contents.iter())
                        }
                        IndexedChestItemData::Object(object) => {
                            chest_items.append(&mut item.contents(&chest.contents));
                            bases.extend(object.info.bases.iter().cloned());
                        }
                        _ => chest_items.append(&mut item.contents(&chest.contents)),
                    }
                }
                ItemContents {
                    chest_items,
                    page_items,
                    bases,
                }
            }
        } else {
            ItemContents {
                chest_items: Vec::new(),
                page_items: Vec::new(),
                bases: Vec::new(),
            }
        }
    }

    /// Searches all chests for items that match a string query. Search is performed within
    /// the given `path`, or all chests if `None`. The result is sorted by relevance, with the
    /// most relevant items first. Empty queries are not supported and return an empty result.
    pub fn search(
        &self,
        path: Option<&ItemPath>,
        query: &str,
        parameters: SearchParameters,
    ) -> Vec<SearchResult> {
        let mut results = Vec::new();
        if let Some(path) = path {
            // Get the chest for the requested identifier
            if let Some(chest) = self.identifiers.get(&path.identifier) {
                // Search the requested chest
                results.extend(
                    chest
                        .contents
                        .search(&path.chest_path, query, &parameters)
                        .into_iter()
                        .map(|result| SearchResult {
                            path: ItemPath {
                                identifier: path.identifier.clone(),
                                chest_path: result.path,
                            },
                            score: result.score,
                        }),
                );
            }
        } else {
            // No path given, search latest version of all chests
            let mut all_contents = Vec::new();
            for versions in self.tags.values() {
                if let Some(identifier) = versions.versions.get(&versions.latest_version) {
                    if let Some(chest) = self.identifiers.get(identifier) {
                        all_contents.push((identifier.as_str(), &chest.contents));
                        all_contents.push((identifier.as_str(), &chest.contents));
                        all_contents.push((identifier.as_str(), &chest.contents));
                        all_contents.push((identifier.as_str(), &chest.contents));
                        all_contents.push((identifier.as_str(), &chest.contents));
                        all_contents.push((identifier.as_str(), &chest.contents));
                        all_contents.push((identifier.as_str(), &chest.contents));
                    }
                }
            }

            results = all_contents
                .par_iter()
                .map(|(identifier, contents)| {
                    contents
                        .search(&ChestPath::root(), query, &parameters)
                        .into_iter()
                        .map(|result| SearchResult {
                            path: ItemPath {
                                identifier: identifier.to_string(),
                                chest_path: result.path,
                            },
                            score: result.score,
                        })
                })
                .flatten_iter()
                .collect();
        }

        // Finalize results by sorting and truncating to the requested count
        results.sort_unstable_by(|a, b| a.cmp(&b));
        results.dedup();
        results.truncate(parameters.result_count);
        results
    }

    /// Gets the user visible tag name for a chest identifier. This will include the
    /// version number if the chest identifier references a version that isn't the latest.
    pub fn tag_for_identifier(&self, identifier: &str) -> Option<String> {
        if let Some(chest) = self.identifiers.get(identifier) {
            if let Some(tag_versions) = self.tags.get(&chest.contents.info.category_tag) {
                if tag_versions.latest_version == chest.contents.info.version {
                    Some(chest.contents.info.category_tag.clone())
                } else {
                    Some(format!(
                        "{}@{}",
                        chest.contents.info.category_tag, chest.contents.info.version
                    ))
                }
            } else {
                Some(format!(
                    "{}@{}",
                    chest.contents.info.category_tag, chest.contents.info.version
                ))
            }
        } else {
            None
        }
    }

    /// Looks up the chest identifier for a given tag name.
    pub fn identifier_for_tag(&self, tag: &str) -> Option<String> {
        let parts = tag.split('@').collect::<Vec<_>>();
        match parts.len() {
            1 => {
                // If no '@' is present, use latest version of the tag
                if let Some(tag_versions) = self.tags.get(parts[0]) {
                    tag_versions
                        .versions
                        .get(&tag_versions.latest_version)
                        .map(|identifier| identifier.clone())
                } else {
                    None
                }
            }
            2 => {
                // If '@' is present, use the version specified
                if let Some(tag_versions) = self.tags.get(parts[0]) {
                    tag_versions
                        .versions
                        .get(parts[1])
                        .map(|identifier| identifier.clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Gets the path corresponding to the item that a URL is pointing to.
    pub fn item_for_path(
        &self,
        identifier: &str,
        url: &str,
        path_hint: Option<&ItemPath>,
    ) -> Option<ItemPath> {
        if let Some(chest) = self.chest(identifier) {
            if let Some(path) = chest.item_for_path(url, path_hint.map(|path| &path.chest_path)) {
                Some(ItemPath {
                    identifier: identifier.to_string(),
                    chest_path: path,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Gets the path corresponding to the page that an item is present on.
    pub fn page_for_path(
        &self,
        identifier: &str,
        url: &str,
        path_hint: Option<&ItemPath>,
    ) -> Option<ItemPath> {
        if let Some(chest) = self.chest(identifier) {
            if let Some(path) = chest.page_for_path(url, path_hint.map(|path| &path.chest_path)) {
                Some(ItemPath {
                    identifier: identifier.to_string(),
                    chest_path: path,
                })
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Reads a file from a chest in the database.
    pub fn read(&self, identifier: &str, path: &str, theme: Theme) -> Result<Vec<u8>> {
        if let Some(chest) = self.identifiers.get(identifier) {
            let path = chest.contents.transform_path_for_theme(path, theme);
            chest.chest.read(&path)
        } else {
            Err(anyhow!("Chest {} not found in database", identifier))
        }
    }

    /// Lists a directory from a chest in the database.
    pub fn list_dir(&self, identifier: &str, path: &str) -> Result<Vec<ChestListEntry>> {
        if let Some(chest) = self.identifiers.get(identifier) {
            chest.chest.list_dir(path)
        } else {
            Err(anyhow!("Chest {} not found in database", identifier))
        }
    }
}

impl SearchParameters {
    pub const DEFAULT_COUNT: usize = 20;
}

impl Default for SearchParameters {
    fn default() -> Self {
        Self {
            result_count: Self::DEFAULT_COUNT,
        }
    }
}

impl PartialOrd for ItemPath {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.chest_path
                .cmp(&other.chest_path)
                .then_with(|| self.identifier.cmp(&other.identifier)),
        )
    }
}

impl Ord for ItemPath {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl PartialOrd for SearchResult {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            other
                .score
                .cmp(&self.score)
                .then_with(|| self.path.cmp(&other.path)),
        )
    }
}

impl Ord for SearchResult {
    fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).unwrap()
    }
}
