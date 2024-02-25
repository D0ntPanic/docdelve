use napi::bindgen_prelude::{Buffer, JsError, Status};
use napi_derive::napi;
use std::collections::BTreeSet;
use std::sync::RwLock;

// Bridge error type for auto-converting anyhow::Error into napi::Error and JsError
pub struct Error(napi::Error);

pub type Result<T> = std::result::Result<T, Error>;

#[napi]
pub struct Database(RwLock<docdelve::db::Database>);

#[napi(object)]
pub struct ChestContents {
    pub name: String,
    pub identifier: String,
    pub items: Vec<ChestPathElement>,
    pub category_tag: String,
    pub extension_module: Option<String>,
    pub version: String,
    pub start_url: String,
    pub light_mode: Option<ThemeAdjustment>,
    pub dark_mode: Option<ThemeAdjustment>,
}

#[napi(object)]
pub struct ChestPathElement {
    pub element_type: ChestItemType,
    pub name: String,
}

#[napi(object)]
pub struct ChestPath {
    pub elements: Vec<ChestPathElement>,
}

#[napi(object)]
pub struct ItemPath {
    pub identifier: String,
    pub chest_path: ChestPath,
}

#[napi(object)]
pub struct ChestItem {
    pub item_type: ChestItemType,
    pub name: String,
    pub full_name: Option<String>,
    pub declaration: Option<String>,
    pub url: Option<String>,
    pub object_type: Option<ObjectType>,
    pub bases: Vec<ChestPath>,
    pub elements: Vec<ChestPathElement>,
    pub page_contents: Vec<PageItem>,
}

#[napi(object)]
pub struct PageItem {
    pub item_type: PageItemType,
    pub title: String,
    pub url: Option<String>,
    pub contents: Vec<PageItem>,
}

#[napi(string_enum)]
pub enum PageItemType {
    Category,
    Link,
}

#[napi(string_enum)]
pub enum ChestItemType {
    Module,
    Group,
    Page,
    Object,
}

#[napi(string_enum)]
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

#[napi(object)]
pub struct SearchResult {
    pub path: ItemPath,
    pub score: u32,
}

#[napi(object)]
pub struct SearchParameters {
    pub result_count: u32,
}

#[napi(object)]
pub struct ChestListEntry {
    pub entry_type: ChestListEntryType,
    pub name: String,
}

#[napi(string_enum)]
pub enum ChestListEntryType {
    File,
    Directory,
}

#[napi(object)]
pub struct ThemeAdjustment {
    pub file_replacements: Vec<FileReplacementRule>,
}

#[napi(object)]
pub struct FileReplacementRule {
    pub pattern: String,
    pub replacement: String,
}

#[napi(string_enum)]
pub enum Theme {
    Light,
    Dark,
}

#[napi(object)]
pub struct ItemContents {
    pub chest_items: Vec<ChestItem>,
    pub page_items: Vec<PageItem>,
    pub bases: Vec<ChestPath>,
}

#[napi]
impl Database {
    #[napi(constructor)]
    pub fn load() -> Result<Self> {
        Ok(Self(RwLock::new(docdelve::db::Database::load()?)))
    }

    #[napi]
    pub fn chest(&self, identifier: String) -> Option<ChestContents> {
        self.0
            .read()
            .unwrap()
            .chest(&identifier)
            .map(|chest| chest.into())
    }

    #[napi]
    pub fn items_at_path(&self, path: ItemPath) -> Vec<ChestItem> {
        let db = self.0.read().unwrap();
        if let Some(chest) = db.chest(&path.identifier) {
            db.items_at_path(&path.into())
                .into_iter()
                .map(|item| ChestItem::from(chest, item))
                .collect()
        } else {
            Vec::new()
        }
    }

    #[napi]
    pub fn item_contents_at_path(&self, path: ItemPath) -> ItemContents {
        let db = self.0.read().unwrap();
        if let Some(chest) = db.chest(&path.identifier) {
            ItemContents::from(chest, db.item_contents_at_path(&path.into()))
        } else {
            ItemContents {
                chest_items: Vec::new(),
                page_items: Vec::new(),
                bases: Vec::new(),
            }
        }
    }

    #[napi]
    pub fn search(
        &self,
        path: Option<ItemPath>,
        query: String,
        parameters: Option<SearchParameters>,
    ) -> Vec<SearchResult> {
        self.0
            .read()
            .unwrap()
            .search(
                path.map(|path| path.into()).as_ref(),
                &query,
                parameters.unwrap_or_default().into(),
            )
            .into_iter()
            .map(|result| result.into())
            .collect()
    }

    #[napi]
    pub fn tag_for_identifier(&self, identifier: String) -> Option<String> {
        self.0.read().unwrap().tag_for_identifier(&identifier)
    }

    #[napi]
    pub fn identifier_for_tag(&self, tag: String) -> Option<String> {
        self.0.read().unwrap().identifier_for_tag(&tag)
    }

    #[napi]
    pub fn page_for_path(
        &self,
        identifier: String,
        url: String,
        path: Option<ItemPath>,
    ) -> Option<ItemPath> {
        self.0
            .read()
            .unwrap()
            .page_for_path(&identifier, &url, path.map(|path| path.into()).as_ref())
            .as_ref()
            .map(|path| path.into())
    }

    #[napi]
    pub fn read(&self, identifier: String, path: String, theme: Theme) -> Result<Buffer> {
        Ok(self
            .0
            .read()
            .unwrap()
            .read(&identifier, &path, theme.into())?
            .into())
    }

    #[napi]
    pub fn list_dir(&self, identifier: String, path: String) -> Result<Vec<ChestListEntry>> {
        Ok(self
            .0
            .read()
            .unwrap()
            .list_dir(&identifier, &path)?
            .iter()
            .map(|entry| entry.into())
            .collect())
    }
}

impl From<&docdelve::content::IndexedChestContents> for ChestContents {
    fn from(contents: &docdelve::content::IndexedChestContents) -> Self {
        Self {
            name: contents.info.name.clone(),
            identifier: contents.info.identifier.clone(),
            items: ChestPathElement::path_elements_for_items(&contents.items()),
            category_tag: contents.info.category_tag.clone(),
            extension_module: contents.info.extension_module.clone(),
            version: contents.info.version.clone(),
            start_url: contents.info.start_url.clone(),
            light_mode: contents.info.light_mode.as_ref().map(|theme| theme.into()),
            dark_mode: contents.info.dark_mode.as_ref().map(|theme| theme.into()),
        }
    }
}

impl From<&docdelve::content::ChestPathElement> for ChestPathElement {
    fn from(element: &docdelve::content::ChestPathElement) -> Self {
        Self {
            element_type: element.element_type.into(),
            name: element.name.clone(),
        }
    }
}

impl From<&ChestPathElement> for docdelve::content::ChestPathElement {
    fn from(element: &ChestPathElement) -> Self {
        Self {
            element_type: element.element_type.into(),
            name: element.name.clone(),
        }
    }
}

impl ChestPathElement {
    fn path_elements_for_items(
        items: &[&docdelve::content::IndexedChestItem],
    ) -> Vec<ChestPathElement> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::new();
        for item in items {
            let element = item.as_path_element();
            if !seen.contains(&element) {
                seen.insert(element.clone());
                result.push(element);
            }
        }
        result.iter().map(|element| element.into()).collect()
    }
}

impl From<docdelve::content::ChestPathElementType> for ChestItemType {
    fn from(element_type: docdelve::content::ChestPathElementType) -> Self {
        match element_type {
            docdelve::content::ChestPathElementType::Module => ChestItemType::Module,
            docdelve::content::ChestPathElementType::Group => ChestItemType::Group,
            docdelve::content::ChestPathElementType::Page => ChestItemType::Page,
            docdelve::content::ChestPathElementType::Object => ChestItemType::Object,
        }
    }
}

impl From<ChestItemType> for docdelve::content::ChestPathElementType {
    fn from(item_type: ChestItemType) -> Self {
        match item_type {
            ChestItemType::Module => docdelve::content::ChestPathElementType::Module,
            ChestItemType::Group => docdelve::content::ChestPathElementType::Group,
            ChestItemType::Page => docdelve::content::ChestPathElementType::Page,
            ChestItemType::Object => docdelve::content::ChestPathElementType::Object,
        }
    }
}

impl From<&docdelve::content::ChestPath> for ChestPath {
    fn from(path: &docdelve::content::ChestPath) -> Self {
        Self {
            elements: path.elements.iter().map(|element| element.into()).collect(),
        }
    }
}

impl From<ChestPath> for docdelve::content::ChestPath {
    fn from(path: ChestPath) -> Self {
        Self {
            elements: path.elements.iter().map(|element| element.into()).collect(),
        }
    }
}

impl From<&docdelve::db::ItemPath> for ItemPath {
    fn from(path: &docdelve::db::ItemPath) -> Self {
        Self {
            identifier: path.identifier.clone(),
            chest_path: (&path.chest_path).into(),
        }
    }
}

impl From<ItemPath> for docdelve::db::ItemPath {
    fn from(path: ItemPath) -> Self {
        Self {
            identifier: path.identifier,
            chest_path: path.chest_path.into(),
        }
    }
}

impl ChestItem {
    fn from(
        chest: &docdelve::content::IndexedChestContents,
        item: &docdelve::content::IndexedChestItem,
    ) -> Self {
        match &item.data {
            docdelve::content::IndexedChestItemData::Module(module) => ChestItem {
                item_type: ChestItemType::Module,
                name: module.info.name.clone(),
                full_name: None,
                declaration: None,
                url: module.info.url.clone(),
                object_type: None,
                bases: Vec::new(),
                elements: ChestPathElement::path_elements_for_items(&item.contents(chest)),
                page_contents: Vec::new(),
            },
            docdelve::content::IndexedChestItemData::Group(group) => ChestItem {
                item_type: ChestItemType::Group,
                name: group.info.name.clone(),
                full_name: None,
                declaration: None,
                url: group.info.url.clone(),
                object_type: None,
                bases: Vec::new(),
                elements: ChestPathElement::path_elements_for_items(&item.contents(chest)),
                page_contents: Vec::new(),
            },
            docdelve::content::IndexedChestItemData::Page(page) => ChestItem {
                item_type: ChestItemType::Page,
                name: page.title.clone(),
                full_name: None,
                declaration: None,
                url: Some(page.url.clone()),
                object_type: None,
                bases: Vec::new(),
                elements: Vec::new(),
                page_contents: page.contents.iter().map(|item| item.into()).collect(),
            },
            docdelve::content::IndexedChestItemData::Object(object) => ChestItem {
                item_type: ChestItemType::Object,
                name: object.info.name.clone(),
                full_name: Some(object.info.full_name.clone()),
                declaration: object.info.declaration.clone(),
                url: object.info.url.clone(),
                object_type: Some(object.info.object_type.into()),
                bases: object.info.bases.iter().map(|base| base.into()).collect(),
                elements: ChestPathElement::path_elements_for_items(&item.contents(chest)),
                page_contents: Vec::new(),
            },
        }
    }
}

impl From<&docdelve::content::PageItem> for PageItem {
    fn from(item: &docdelve::content::PageItem) -> Self {
        match item {
            docdelve::content::PageItem::Category(category) => PageItem {
                item_type: PageItemType::Category,
                title: category.title.clone(),
                url: category.url.clone(),
                contents: category.contents.iter().map(|item| item.into()).collect(),
            },
            docdelve::content::PageItem::Link(link) => PageItem {
                item_type: PageItemType::Link,
                title: link.title.clone(),
                url: Some(link.url.clone()),
                contents: Vec::new(),
            },
        }
    }
}

impl From<docdelve::content::ObjectType> for ObjectType {
    fn from(object_type: docdelve::content::ObjectType) -> Self {
        match object_type {
            docdelve::content::ObjectType::Class => ObjectType::Class,
            docdelve::content::ObjectType::Struct => ObjectType::Struct,
            docdelve::content::ObjectType::Union => ObjectType::Union,
            docdelve::content::ObjectType::Object => ObjectType::Object,
            docdelve::content::ObjectType::Enum => ObjectType::Enum,
            docdelve::content::ObjectType::Value => ObjectType::Value,
            docdelve::content::ObjectType::Variant => ObjectType::Variant,
            docdelve::content::ObjectType::Trait => ObjectType::Trait,
            docdelve::content::ObjectType::TraitImplementation => ObjectType::TraitImplementation,
            docdelve::content::ObjectType::Interface => ObjectType::Interface,
            docdelve::content::ObjectType::Function => ObjectType::Function,
            docdelve::content::ObjectType::Method => ObjectType::Method,
            docdelve::content::ObjectType::Variable => ObjectType::Variable,
            docdelve::content::ObjectType::Member => ObjectType::Member,
            docdelve::content::ObjectType::Field => ObjectType::Field,
            docdelve::content::ObjectType::Constant => ObjectType::Constant,
            docdelve::content::ObjectType::Property => ObjectType::Property,
            docdelve::content::ObjectType::Typedef => ObjectType::Typedef,
            docdelve::content::ObjectType::Namespace => ObjectType::Namespace,
        }
    }
}

impl From<docdelve::db::SearchResult> for SearchResult {
    fn from(result: docdelve::db::SearchResult) -> Self {
        Self {
            path: (&result.path).into(),
            score: result.score as u32,
        }
    }
}

impl From<docdelve::db::SearchParameters> for SearchParameters {
    fn from(parameters: docdelve::db::SearchParameters) -> Self {
        Self {
            result_count: parameters.result_count as u32,
        }
    }
}

impl From<SearchParameters> for docdelve::db::SearchParameters {
    fn from(parameters: SearchParameters) -> Self {
        Self {
            result_count: parameters.result_count as usize,
        }
    }
}

impl Default for SearchParameters {
    fn default() -> Self {
        docdelve::db::SearchParameters::default().into()
    }
}

impl From<&docdelve::chest::ChestListEntry> for ChestListEntry {
    fn from(entry: &docdelve::chest::ChestListEntry) -> Self {
        match entry {
            docdelve::chest::ChestListEntry::File(name) => ChestListEntry {
                entry_type: ChestListEntryType::File,
                name: name.clone(),
            },
            docdelve::chest::ChestListEntry::Directory(name) => ChestListEntry {
                entry_type: ChestListEntryType::Directory,
                name: name.clone(),
            },
        }
    }
}

impl From<&docdelve::content::ThemeAdjustment> for ThemeAdjustment {
    fn from(theme: &docdelve::content::ThemeAdjustment) -> Self {
        Self {
            file_replacements: theme
                .file_replacements
                .iter()
                .map(|replacement| replacement.into())
                .collect(),
        }
    }
}

impl From<&docdelve::content::FileReplacementRule> for FileReplacementRule {
    fn from(rule: &docdelve::content::FileReplacementRule) -> Self {
        Self {
            pattern: rule.pattern.clone(),
            replacement: rule.replacement.clone(),
        }
    }
}

impl From<Theme> for docdelve::db::Theme {
    fn from(theme: Theme) -> Self {
        match theme {
            Theme::Light => docdelve::db::Theme::Light,
            Theme::Dark => docdelve::db::Theme::Dark,
        }
    }
}

impl ItemContents {
    fn from(
        chest: &docdelve::content::IndexedChestContents,
        contents: docdelve::db::ItemContents,
    ) -> Self {
        ItemContents {
            chest_items: contents
                .chest_items
                .into_iter()
                .map(|item| ChestItem::from(chest, item))
                .collect(),
            page_items: contents
                .page_items
                .into_iter()
                .map(|item| item.into())
                .collect(),
            bases: contents.bases.iter().map(|base| base.into()).collect(),
        }
    }
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error(napi::Error::new(Status::GenericFailure, &err.to_string()))
    }
}

impl From<Error> for napi::Error {
    fn from(err: Error) -> Self {
        err.0
    }
}

impl From<Error> for JsError {
    fn from(err: Error) -> Self {
        napi::JsError::from(err.0)
    }
}
