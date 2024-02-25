use anyhow::{Error, Result};
use diffy::Patch;
use docdelve::chest::{Chest, ChestListEntry};
use docdelve::container::{Container, ContainerEngine};
use docdelve::content::{
    ChestContents, ChestItem, ChestPath, ChestPathElement, ChestPathElementType,
    FileReplacementRule, Group, GroupInfo, Module, ModuleInfo, Object, ObjectInfo, ObjectType,
    Page, PageCategory, PageItem, PageLink, ThemeAdjustment,
};
use docdelve::progress::ProgressEvent;
use regex::Regex;
use roxmltree::ParsingOptions;
use std::collections::BTreeMap;

/// URL of the main repository for Qt
const QT_GIT_URL: &'static str = "git://code.qt.io/qt/qt5.git";

/// Generator for Qt documentation
pub struct QtDocumentationGenerator {
    container: Container,
    version: String,
    name_filter_regex: Regex,
}

struct QMLModule {
    url: Option<String>,
    modules: BTreeMap<String, Box<QMLModule>>,
    classes: Vec<Object>,
}

struct ResolvedBase {
    path: ChestPath,
    bases: Vec<ChestPath>,
}

impl QtDocumentationGenerator {
    /// Create a Qt documentation generator for the given version of Qt
    pub fn new(engine: ContainerEngine, version: &str) -> Result<Self> {
        // Validate version string
        if !Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+(\.[0-9]+)?(-[a-zA-Z0-9]+)?$")
            .unwrap()
            .is_match(version)
        {
            return Err(Error::msg("Invalid Qt version"));
        }

        let mut container = Container::new(engine);

        // Install required packages
        container.apt_install(&[
            "build-essential",
            "git",
            "python3-html5lib",
            "cmake",
            "ninja-build",
            "ca-certificates",
            "llvm-14-dev",
            "libclang-14-dev",
            "clang-14",
            "clang-tools-14",
            "nodejs",
            "gperf",
            "flex",
            "bison",
            "zip",
            "mesa-common-dev",
            "libgl1-mesa-dev",
            "libglu1-mesa-dev",
            "libicu-dev",
            "libdbus-1-dev",
            "libfontconfig1-dev",
            "libfreetype6-dev",
            "libx11-dev",
            "libx11-xcb-dev",
            "libxext-dev",
            "libxfixes-dev",
            "libxi-dev",
            "libxrender-dev",
            "libxcb1-dev",
            "libxcb-glx0-dev",
            "libxcb-keysyms1-dev",
            "libxcb-image0-dev",
            "libxcb-shm0-dev",
            "libxcb-icccm4-dev",
            "libxcb-sync0-dev",
            "libxcb-xfixes0-dev",
            "libxcb-shape0-dev",
            "libxcb-randr0-dev",
            "libxcb-render-util0-dev",
            "libxcb-xinerama0-dev",
            "libxkbcommon-dev",
            "libxkbcommon-x11-dev",
            "libxcb-cursor-dev",
            "libxcomposite-dev",
            "libxcursor-dev",
            "libxrandr-dev",
            "libxshmfence-dev",
            "libxtst-dev",
            "libxdamage-dev",
            "libnss3-dev",
            "libxkbfile-dev",
            "libsecret-1-dev",
            "libcups2-dev",
            "libsqlite3-dev",
            "libssl-dev",
            "libwayland-dev",
        ]);

        // Place libclang into the PATH so that Qt can find it
        container.env("PATH", "$PATH:/usr/lib/llvm-14/bin");

        // Set locale to UTF-8. This is required by Qt.
        container.env("LANG", "C.UTF-8");

        // Clone the main Qt repository
        container.command(&["git", "clone", QT_GIT_URL, "/source"]);
        container.git_clone_progress("Qt");
        container.work_dir("/source");

        // Checkout the requested version
        container.command(&["git", "checkout", &format!("v{}", version)]);

        // Initialize the submodules in the Qt repository
        container.command(&["./init-repository", "--no-update"]);
        container.git_submodule_progress();

        // Download the Qt submodules
        container.command(&[
            "git",
            "submodule",
            "update",
            "--init",
            "--recursive",
            "--no-recommend-shallow",
            "--depth=1",
        ]);
        container.git_submodule_progress();

        // Configure Qt build
        container.command(&[
            "./configure",
            "-no-static",
            "-release",
            "-opensource",
            "-confirm-license",
        ]);
        container.generic_progress("Configuring Qt build");

        // Need to build qminimal platform. This is required to complete the documentation build, but the
        // Qt build scripts don't add it as a dependency.
        container.command(&["ninja", "qminimal"]);
        container.ninja_build_progress("minimal Qt platform");

        // Need to build qsqlite driver. This is required to complete the documentation build, but the
        // Qt build scripts don't add it as a dependency.
        container.command(&["ninja", "qsqlite"]);
        container.ninja_build_progress("documentation database dependencies");

        // Build the documentation
        container.command(&["ninja", "docs"]);
        container.ninja_build_progress("Qt documentation");

        // Remove .qch files as they will not be needed
        container.command(&["/bin/bash", "-c", "rm -f /source/doc/*.qch"]);

        Ok(Self {
            container,
            version: version.to_string(),
            name_filter_regex: Regex::new(r"</?@[^>]*>").unwrap(),
        })
    }

    /// Build the Qt documentation
    pub fn build<F>(&mut self, mut progress: F) -> Result<()>
    where
        F: FnMut(ProgressEvent),
    {
        // Build the Qt documentation image
        self.container.build(&mut progress)?;

        // Extract the built documentation from the image
        progress(ProgressEvent::Action(
            "Loading built Qt documentation".into(),
        ));
        let mut chest = self.container.get_archive("/source/doc")?;

        // Patch the dark mode CSS to fix target color highlighting
        const DARK_MODE_PATCH_STR: &str = include_str!("./qt_dark_mode.patch");
        let patch = Patch::from_str(DARK_MODE_PATCH_STR)?;
        for file in chest.find_all("offline-dark.css") {
            chest.patch(&file, &patch)?;
        }

        // Create the chest contents
        let mut contents = ChestContents::new(
            "Qt",
            "qt",
            None,
            &self.version,
            "qtdoc/index.html",
            Some(ThemeAdjustment {
                file_replacements: vec![FileReplacementRule {
                    pattern: "offline-simple.css".into(),
                    replacement: "offline.css".into(),
                }],
            }),
            Some(ThemeAdjustment {
                file_replacements: vec![
                    FileReplacementRule {
                        pattern: "offline-simple.css".into(),
                        replacement: "offline-dark.css".into(),
                    },
                    FileReplacementRule {
                        pattern: "offline.css".into(),
                        replacement: "offline-dark.css".into(),
                    },
                ],
            }),
        );

        // Iterate through the modules in the documentation and enumerate the contents
        for entry in chest.list_dir("/")? {
            match entry {
                ChestListEntry::Directory(name) => {
                    progress(ProgressEvent::Action(format!(
                        "Generating chest contents for {}",
                        name
                    )));
                    self.add_module_contents(
                        &mut chest,
                        &mut contents,
                        &name,
                        &format!("{}/", name),
                    )?;
                }
                _ => (),
            }
        }

        progress(ProgressEvent::Action("Finalizing chest".into()));

        let mut resolved_bases = Vec::new();
        let mut path = Vec::new();
        self.resolve_base_classes(&contents, &contents.items, &mut path, &mut resolved_bases);
        for base in resolved_bases {
            self.apply_resolved_base_class(&mut contents.items, &base.path.elements, base.bases);
        }

        // Save the chest contents into the chest
        contents.write_to_chest(&mut chest)?;

        // Save the built documentation chest
        chest.save(
            &std::path::Path::new(&format!("qt-docs-{}.ddchest", self.version)),
            &mut progress,
        )?;

        Ok(())
    }

    /// Adds the contents of a module within the Qt documentation chest
    fn add_module_contents(
        &self,
        chest: &mut Chest,
        contents: &mut ChestContents,
        name: &str,
        url_prefix: &str,
    ) -> Result<()> {
        // Parse the index for the module and remove the original XML index from the chest
        let index = String::from_utf8(chest.read(&format!("{}/{}.index", name, name))?)?;
        let index = roxmltree::Document::parse_with_options(
            &index,
            ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )?;
        chest.remove(&format!("{}/{}.index", name, name))?;

        // Get the root index node
        let index_node = index
            .root()
            .first_child()
            .ok_or_else(|| Error::msg("No index node"))?;
        if index_node.tag_name().name() != "INDEX" {
            return Err(Error::msg("Invalid index node"));
        }

        // Grab the project name from the index node and create a module for it
        let project = index_node
            .attribute_node("project")
            .ok_or_else(|| Error::msg("No project attribute on index"))?;
        let project_name = project.value();
        let project_index_url = format!("{}/{}-index.html", name, name);
        let mut module = Module {
            info: ModuleInfo {
                name: project_name.to_string(),
                full_name: project_name.to_string(),
                url: if chest.contains(&project_index_url) {
                    Some(project_index_url)
                } else {
                    None
                },
            },
            contents: Vec::new(),
        };

        // Look for the namespace node within the index node and add the nodes within it
        let mut qml_classes = Vec::new();
        let mut qml_modules = Vec::new();
        for node in index_node.children() {
            if !node.is_element() {
                continue;
            }

            if node.tag_name().name() == "namespace" {
                self.add_nodes(
                    &mut module.contents,
                    &node,
                    "",
                    url_prefix,
                    None,
                    &mut qml_classes,
                    &mut qml_modules,
                )?;
            } else {
                return Err(Error::msg("Expected namespace node"));
            }
        }

        // Resolve any pending QML modules and classes
        self.add_qml_classes(&mut module, qml_classes, qml_modules);

        contents.items.push(ChestItem::Module(Box::new(module)));
        Ok(())
    }

    /// Parses an index node and adds the contents to the list of items in the chest
    fn add_nodes(
        &self,
        contents: &mut Vec<ChestItem>,
        node: &roxmltree::Node,
        namespace: &str,
        url_prefix: &str,
        parent_url: Option<&str>,
        qml_classes: &mut Vec<Object>,
        qml_modules: &mut Vec<Object>,
    ) -> Result<()> {
        for node in node.children() {
            if !node.is_element() {
                // Index files only have elements
                continue;
            }

            // Macro for fetching a required attribute string
            macro_rules! required_attr {
                ($name:expr) => {
                    node.attribute_node($name)
                        .ok_or_else(|| Error::msg(format!("Missing required attribute {}", $name)))?
                        .value()
                        .to_string()
                };
            }

            // Macro for fetching an optional attribute string
            macro_rules! optional_attr {
                ($name:expr) => {
                    node.attribute_node($name)
                        .map(|attr| attr.value().to_string())
                };
            }

            // Macro for fetching a required URL
            macro_rules! required_url {
                () => {
                    url_prefix.to_string() + &required_attr!("href")
                };
            }

            // Macro for fetching an optional URL
            macro_rules! optional_url {
                () => {
                    optional_attr!("href").map(|url| url_prefix.to_string() + &url)
                };
            }

            // Macro for adding a single, non-recursive object
            macro_rules! named_single_object {
                ($object_type: expr) => {
                    let name = self.filter_name(required_attr!("name"));
                    let full_name = format!("{}{}", namespace, name);
                    contents.push(ChestItem::Object(Box::new(Object {
                        info: ObjectInfo {
                            name,
                            full_name,
                            declaration: optional_attr!("signature"),
                            url: optional_url!(),
                            object_type: $object_type,
                            bases: Vec::new(),
                        },
                        contents: Vec::new(),
                    })));
                };
            }

            // Macro for adding a recursive object
            macro_rules! named_recursive_object {
                ($object_type: expr) => {
                    let name = self.filter_name(required_attr!("name"));
                    let full_name = format!("{}{}", namespace, name);

                    let bases: Vec<ChestPath> = if let Some(base_name_str) = optional_attr!("bases")
                    {
                        base_name_str
                            .split(',')
                            .map(|base_name| ChestPath {
                                elements: vec![ChestPathElement {
                                    element_type: ChestPathElementType::Object,
                                    name: base_name.to_string(),
                                }],
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };

                    let mut obj = Object {
                        info: ObjectInfo {
                            name,
                            full_name: full_name.clone(),
                            declaration: None,
                            url: optional_url!(),
                            object_type: $object_type,
                            bases,
                        },
                        contents: Vec::new(),
                    };
                    self.add_nodes(
                        &mut obj.contents,
                        &node,
                        &format!("{}::", full_name),
                        url_prefix,
                        obj.info.url.as_ref().map(|url| url.as_str()),
                        qml_classes,
                        qml_modules,
                    )?;
                    contents.push(ChestItem::Object(Box::new(obj)));
                };
            }

            macro_rules! recurse_contents {
                ($obj: expr, $namespace: expr) => {
                    self.add_nodes(
                        &mut $obj.contents,
                        &node,
                        $namespace,
                        url_prefix,
                        None,
                        qml_classes,
                        qml_modules,
                    )?;
                };
            }

            // Add item based on the element type
            match node.tag_name().name() {
                "class" => {
                    named_recursive_object!(ObjectType::Class);
                }
                "struct" => {
                    named_recursive_object!(ObjectType::Struct);
                }
                "union" => {
                    named_recursive_object!(ObjectType::Union);
                }
                "enum" => {
                    named_recursive_object!(ObjectType::Enum);
                }
                "function" => {
                    named_single_object!(ObjectType::Function);
                }
                "typedef" => {
                    named_single_object!(ObjectType::Typedef);
                }
                "value" => {
                    let name = self.filter_name(required_attr!("name"));
                    let full_name = format!("{}{}", namespace, name);
                    contents.push(ChestItem::Object(Box::new(Object {
                        info: ObjectInfo {
                            name,
                            full_name,
                            declaration: optional_attr!("signature"),
                            url: if let Some(url) = optional_url!() {
                                Some(url)
                            } else {
                                parent_url.map(|url| url.to_string())
                            },
                            object_type: ObjectType::Value,
                            bases: Vec::new(),
                        },
                        contents: Vec::new(),
                    })));
                }
                "variable" => {
                    let is_static = required_attr!("static");
                    named_single_object!(if is_static == "true" {
                        ObjectType::Variable
                    } else {
                        ObjectType::Member
                    });
                }
                "namespace" => {
                    let name = required_attr!("name");
                    let full_name = format!("{}{}", namespace, name);
                    let mut namespace = Object {
                        info: ObjectInfo {
                            name,
                            full_name: full_name.clone(),
                            declaration: None,
                            url: optional_url!(),
                            object_type: ObjectType::Namespace,
                            bases: Vec::new(),
                        },
                        contents: Vec::new(),
                    };
                    recurse_contents!(namespace, &format!("{}::", full_name));
                    contents.push(ChestItem::Object(Box::new(namespace)));
                }
                "header" => {
                    let name = required_attr!("name");
                    let mut group = Group {
                        info: GroupInfo {
                            name,
                            url: optional_url!(),
                        },
                        contents: Vec::new(),
                    };
                    recurse_contents!(group, namespace);
                    contents.push(ChestItem::Group(Box::new(group)));
                }
                "page" => {
                    let url = required_url!();
                    if !url.contains(':') {
                        let mut page = Page {
                            title: self.filter_name(required_attr!("title")),
                            url,
                            contents: Vec::new(),
                        };
                        self.add_page_nodes(&mut page.contents, &node, &page.url)?;
                        contents.push(ChestItem::Page(Box::new(page)));
                    }
                }
                "module" => {
                    let url = required_url!();
                    if !url.contains(':') {
                        let mut page = Page {
                            title: self.filter_name(required_attr!("title")),
                            url,
                            contents: Vec::new(),
                        };
                        self.add_page_nodes(&mut page.contents, &node, &page.url)?;
                        contents.push(ChestItem::Page(Box::new(page)));
                    }
                }
                "qmlclass" | "qmlvaluetype" => {
                    // QML classes use fully qualified names and are not placed into module nodes. We
                    // will collect them all first and then resolve the structure later.
                    let name = if let Some(name) = optional_attr!("fullname") {
                        name
                    } else {
                        required_attr!("name")
                    };
                    let mut bases = Vec::new();
                    if let Some(base) = optional_attr!("qml-base-type") {
                        bases.push(ChestPath {
                            elements: vec![ChestPathElement {
                                element_type: ChestPathElementType::Object,
                                name: base.replace("::", "."),
                            }],
                        });
                    }
                    let mut obj = Object {
                        info: ObjectInfo {
                            name: name.clone(),
                            full_name: name.clone(),
                            declaration: None,
                            url: optional_url!(),
                            object_type: ObjectType::Class,
                            bases,
                        },
                        contents: Vec::new(),
                    };
                    recurse_contents!(obj, &format!("{}.", name));
                    qml_classes.push(obj);
                }
                "qmlmodule" => {
                    // QML modules do not contain their contents. Collect the list of modules and they
                    // will be resolved later.
                    let name = required_attr!("name");
                    qml_modules.push(Object {
                        info: ObjectInfo {
                            name: name.clone(),
                            full_name: name,
                            declaration: None,
                            url: optional_url!(),
                            object_type: ObjectType::Namespace,
                            bases: Vec::new(),
                        },
                        contents: Vec::new(),
                    });
                }
                _ => (),
            }
        }
        Ok(())
    }

    /// Parses an index node for a single page and adds the table of contents items
    fn add_page_nodes(
        &self,
        contents: &mut Vec<PageItem>,
        node: &roxmltree::Node,
        url_prefix: &str,
    ) -> Result<()> {
        let mut stack: Vec<PageCategory> = Vec::new();

        // Macro to clear table of contents stack to a given level. This will place the stack in
        // a state ready for insertion of a new item at the given level. The stack can be fully
        // cleared for completion by passing a level of zero.
        macro_rules! clear_stack_to_level {
            ($level:expr) => {
                while $level < stack.len() {
                    let item = stack
                        .pop()
                        .ok_or_else(|| Error::msg("Stack underflow managing table of contents"))?;

                    let item = if item.contents.is_empty() {
                        PageItem::Link(PageLink {
                            title: self.filter_name(item.title),
                            url: item
                                .url
                                .ok_or_else(|| Error::msg("Expected URL in table of contents"))?,
                        })
                    } else {
                        PageItem::Category(Box::new(item))
                    };

                    let mut last = stack.last_mut();
                    if let Some(last) = last.as_mut() {
                        last.contents.push(item);
                    } else {
                        contents.push(item);
                    }
                }
            };
        }

        for node in node.children() {
            if !node.is_element() {
                // Index files only have elements
                continue;
            }

            // Macro for fetching a required attribute string
            macro_rules! required_attr {
                ($name:expr) => {
                    node.attribute_node($name)
                        .ok_or_else(|| Error::msg(format!("Missing required attribute {}", $name)))?
                        .value()
                        .to_string()
                };
            }

            // Macro for fetching an optional attribute string
            macro_rules! optional_attr {
                ($name:expr) => {
                    node.attribute_node($name)
                        .map(|attr| attr.value().to_string())
                };
            }

            // Add item based on the element type
            match node.tag_name().name() {
                "keyword" => {
                    let name = self.filter_name(required_attr!("name"));
                    let title = optional_attr!("title");
                    if let Some(title) = title {
                        contents.push(PageItem::Link(PageLink {
                            title,
                            url: format!("{}#{}", url_prefix, name),
                        }));
                    }
                }
                "contents" => {
                    let name = self.filter_name(required_attr!("name"));
                    let title = required_attr!("title");
                    let level = required_attr!("level").parse::<usize>()? - 1;

                    // Get the stack to the correct level and push the new item
                    clear_stack_to_level!(level);
                    stack.push(PageCategory {
                        title,
                        url: Some(format!("{}#{}", url_prefix, name)),
                        contents: Vec::new(),
                    });
                }
                _ => (),
            }
        }

        // Clear out all stack entries and place them into the final contents
        clear_stack_to_level!(0);
        Ok(())
    }

    /// Resolves QML modules and classes into a tree structure
    fn add_qml_classes(
        &self,
        module: &mut Module,
        qml_classes: Vec<Object>,
        qml_modules: Vec<Object>,
    ) {
        // Construct the module tree containing all modules
        let mut root_module = QMLModule {
            url: None,
            modules: BTreeMap::new(),
            classes: Vec::new(),
        };
        for module in qml_modules {
            self.insert_qml_module(&mut root_module, module);
        }

        // Add classes into the module tree
        for qml_class in qml_classes {
            self.insert_qml_class(&mut root_module, qml_class);
        }

        self.resolve_qml_modules(&mut module.contents, root_module, "");
    }

    /// Inserts a QML module into the module tree structure
    fn insert_qml_module(&self, node: &mut QMLModule, mut module: Object) {
        let parts: Vec<&str> = module.info.name.split('.').collect();
        if parts.is_empty() || parts[0].is_empty() {
            // Stop when no more module names. Add the module's URL to the final node.
            node.url = module.info.url;
            return;
        }

        // Recurse into submodules and add entries to the module tree
        let submodule_name = parts[0].to_string();
        module.info.name = parts[1..].join(".");
        let node = node.modules.entry(submodule_name).or_insert_with(|| {
            Box::new(QMLModule {
                url: None,
                modules: BTreeMap::new(),
                classes: Vec::new(),
            })
        });
        self.insert_qml_module(node, module);
    }

    /// Inserts a QML module into the module tree structure
    fn insert_qml_class(&self, node: &mut QMLModule, mut qml_class: Object) {
        let parts: Vec<&str> = qml_class.info.name.split('.').collect();
        if parts.len() <= 1 {
            // Don't recurse for class name
            node.classes.push(qml_class);
            return;
        } else if let Some(module) = node.modules.get_mut(parts[0]) {
            // Module found, recurse into the next module
            qml_class.info.name = parts[1..].join(".");
            self.insert_qml_class(module, qml_class);
        } else {
            // Module not defined, insert class into last module
            node.classes.push(qml_class);
        }
    }

    /// Resolve QML module tree into chest items
    fn resolve_qml_modules(
        &self,
        contents: &mut Vec<ChestItem>,
        node: QMLModule,
        name_prefix: &str,
    ) {
        // Add classes into the list for the current module
        for qml_class in node.classes {
            contents.push(ChestItem::Object(Box::new(qml_class)));
        }

        // Recurse into submodules and add the modules into the list
        for (name, qml_module) in node.modules {
            let mut module = Object {
                info: ObjectInfo {
                    name: name.clone(),
                    full_name: name_prefix.to_string() + &name,
                    declaration: None,
                    url: qml_module.url.clone(),
                    object_type: ObjectType::Namespace,
                    bases: Vec::new(),
                },
                contents: Vec::new(),
            };

            self.resolve_qml_modules(
                &mut module.contents,
                *qml_module,
                &format!("{}.", module.info.full_name),
            );

            contents.push(ChestItem::Object(Box::new(module)));
        }
    }

    /// Resolves base class names into chest paths
    fn resolve_base_classes(
        &self,
        root: &ChestContents,
        contents: &Vec<ChestItem>,
        path: &mut Vec<ChestPathElement>,
        result: &mut Vec<ResolvedBase>,
    ) {
        for item in contents {
            path.push(item.as_path_element());

            if let ChestItem::Object(object) = item {
                if !object.info.bases.is_empty() {
                    let mut bases = Vec::new();
                    for base in object.info.bases.iter() {
                        if let Some(base_name) = base.elements.first() {
                            let mut find_path = Vec::new();
                            if let Some(base) =
                                self.find_base_class(&root.items, &mut find_path, &base_name.name)
                            {
                                bases.push(base);
                            }
                        }
                    }
                    result.push(ResolvedBase {
                        path: ChestPath {
                            elements: path.clone(),
                        },
                        bases,
                    })
                }
            }

            self.resolve_base_classes(root, item.contents(), path, result);
            path.pop();
        }
    }

    /// Finds a base class path by name
    fn find_base_class(
        &self,
        contents: &Vec<ChestItem>,
        path: &mut Vec<ChestPathElement>,
        name: &str,
    ) -> Option<ChestPath> {
        for item in contents {
            path.push(item.as_path_element());

            if let ChestItem::Object(object) = item {
                if object.info.name == name || object.info.full_name == name {
                    return Some(ChestPath {
                        elements: path.clone(),
                    });
                }
            }

            if let Some(base) = self.find_base_class(item.contents(), path, name) {
                return Some(base);
            }

            path.pop();
        }
        None
    }

    /// Applies a resolved base class path to the chest contents
    fn apply_resolved_base_class(
        &self,
        contents: &mut [ChestItem],
        path: &[ChestPathElement],
        bases: Vec<ChestPath>,
    ) {
        if let Some(element) = path.first() {
            for item in contents.iter_mut() {
                if item.matches_path_element(element) {
                    let rest = &path[1..];
                    if rest.is_empty() {
                        if let ChestItem::Object(object) = item {
                            object.info.bases = bases;
                        }
                    } else if let Some(mut contents) = item.contents_mut() {
                        self.apply_resolved_base_class(&mut contents, rest, bases);
                    }
                    return;
                }
            }
        }
    }

    fn filter_name(&self, name: String) -> String {
        self.name_filter_regex.replace_all(&name, "").to_string()
    }
}
