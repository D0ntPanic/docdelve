use anyhow::{anyhow, Error, Result};
use docdelve::chest::{Chest, ChestListEntry};
use docdelve::container::{Container, ContainerEngine};
use docdelve::content::{ChestContents, ChestItem, Page, PageCategory, PageItem, PageLink};
use docdelve::progress::ProgressEvent;
use regex::Regex;
use scraper::{ElementRef, Html, Node, Selector};

pub struct StandardLibraryDocumentationGenerator {
    container: Container,
    version: String,
}

impl StandardLibraryDocumentationGenerator {
    /// Create a Rust standard library documentation generator for the given version of Rust
    pub fn new(engine: ContainerEngine, version: &str) -> Result<Self> {
        // Validate version string
        if !Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$")?.is_match(version) {
            return Err(Error::msg("Invalid Rust version"));
        }

        let mut container = Container::new(engine);

        // Install required packages
        container.apt_install(&["curl", "ca-certificates"]);

        // Download the Rust installation script
        container.command(&["sh", "-c", "curl https://sh.rustup.rs -sSf > rustup.sh"]);
        container.generic_progress("Downloading Rust installer");

        // Use the installation script to install `rustup`
        container.command(&["chmod", "755", "rustup.sh"]);
        container.command(&["./rustup.sh", "-y"]);
        container.generic_progress("Installing Rust");

        // Place Rust into the PATH so that can use `rustup`
        container.env("PATH", "$PATH:/root/.cargo/bin");

        // Install the requested version of Rust
        container.command(&["rustup", "toolchain", "install", version]);
        container.generic_progress(&format!(
            "Installing Rust toolchain for version {}",
            version
        ));

        // Instead of guessing the correct triple for whatever platform the container
        // is running on, use the shell to symlink the correct toolchain to a known path.
        container.command(&[
            "sh",
            "-c",
            &format!("ln -s /root/.rustup/toolchains/{}-* /toolchain", version),
        ]);

        Ok(Self {
            container,
            version: version.to_string(),
        })
    }

    /// Build the Rust documentation
    pub fn build<F>(&mut self, mut progress: F) -> Result<()>
    where
        F: FnMut(ProgressEvent),
    {
        // Build the Rust documentation image
        self.container.build(&mut progress)?;

        // Extract the built documentation from the image
        progress(ProgressEvent::Action(
            "Loading built Rust documentation".into(),
        ));
        let mut chest = self
            .container
            .get_archive("/toolchain/share/doc/rust/html")?;

        // Create the chest contents
        progress(ProgressEvent::Action("Indexing Rust documentation".into()));
        let mut contents = ChestContents::new(
            "Rust",
            &["rs"],
            None,
            &self.version,
            "index.html",
            None,
            None,
        );

        // Add initial landing page to the chest
        contents.items.push(ChestItem::Page(Box::new(Page {
            title: "Rust Documentation".into(),
            url: "index.html".to_string(),
            contents: Vec::new(),
        })));

        // Add the Rust books to the chest
        Self::add_book(
            &chest,
            &mut contents,
            "book",
            "The Rust Programming Language",
        )?;
        Self::add_book(
            &chest,
            &mut contents,
            "embedded-book",
            "The Embedded Rust Book",
        )?;

        Self::add_book(&chest, &mut contents, "rust-by-example", "Rust By Example")?;
        Self::add_book(&chest, &mut contents, "rustc", "The rustc Book")?;
        Self::add_book(&chest, &mut contents, "cargo", "The Cargo Book")?;
        Self::add_book(&chest, &mut contents, "rustdoc", "The Rustdoc Book")?;
        Self::add_book(&chest, &mut contents, "clippy", "The Clippy Book")?;
        Self::add_book(&chest, &mut contents, "error_codes", "rustc error codes")?;
        Self::add_book(&chest, &mut contents, "reference", "The Reference")?;
        Self::add_book(&chest, &mut contents, "style-guide", "The Rust Style Guide")?;
        Self::add_book(&chest, &mut contents, "nomicon", "The Rustonomicon")?;
        Self::add_book(&chest, &mut contents, "unstable-book", "The Unstable Book")?;

        // Patch CSS to remove sidebars and search, as these are provided by the app itself.
        for file in chest.list_dir("static.files")? {
            if let ChestListEntry::File(file) = file {
                if file.starts_with("rustdoc-") && file.ends_with(".css") {
                    let mut css =
                        String::from_utf8(chest.read(&format!("static.files/{}", file))?)?;
                    css.push_str("\n.sidebar { display: none; }\n");
                    css.push_str(".search-form { display: none; }\n");
                    chest.write(&format!("static.files/{}", file), css.as_bytes())?;
                }
            }
        }

        for path in chest.find_all("chrome.css") {
            let mut css = String::from_utf8(chest.read(&path)?)?;
            css.push_str("#menu-bar { display: none; }\n");
            chest.write(&path, css.as_bytes())?;
        }

        // Fix up CSS used by `index.html` to include theme handling and remove the
        // unneeded search form.
        let mut css = String::from_utf8(chest.read("rust.css")?)?;
        css.push_str(
            "@media (prefers-color-scheme: dark) {
                body {
                    background-color: #181818;
                    color: #e0e0e0;
                }
                h1, h2, h3, h4, h5, h6, h1 a:link, h1 a:visited, h2 a:link, h2 a:visited,
                h3 a:link, h3 a:visited, h4 a:link, h4 a:visited, h5 a:link, h5:visited,
                h6 a:link, h6 a:visited, code {
                    color: #e0e0e0;
                }
                form {
                    display: none;
                }
            }\n",
        );
        chest.write("rust.css", css.as_bytes())?;

        // Save the chest contents into the chest
        contents.write_to_chest(&mut chest)?;

        // Save the built documentation chest
        chest.save(
            &std::path::Path::new(&format!("rust-stdlib-{}.ddchest", self.version)),
            &mut progress,
        )?;

        Ok(())
    }

    /// Indexes the contents of a book and adds it to the chest
    fn add_book(
        chest: &Chest,
        contents: &mut ChestContents,
        path: &str,
        title: &str,
    ) -> Result<()> {
        // Load and parse the HTML of the initial page, which contains a sidebar with all
        // the other pages referenced.
        let html_str = String::from_utf8(chest.read(&format!("{}/{}", path, "index.html"))?)?;
        let html = Html::parse_document(&html_str);

        // Find the sidebar content element
        let sidebar = html
            .select(&Self::selector(".sidebar")?)
            .next()
            .ok_or_else(|| anyhow!("Could not find sidebar in '{}'", title))?;
        let sidebar_contents = sidebar
            .select(&Self::selector("ol")?)
            .next()
            .ok_or_else(|| anyhow!("Could not find sidebar contents in '{}'", title))?;

        // Parse the content tree of the book from the sidebar contents
        let pages = Self::collect_book_pages(path, sidebar_contents)?;

        // Add the pages to the chest
        contents.items.push(ChestItem::Page(Box::new(Page {
            title: title.into(),
            url: format!("{}/index.html", path),
            contents: pages,
        })));
        Ok(())
    }

    /// Collects the page hierarchy for an element in a book's sidebar
    fn collect_book_pages(path: &str, element: ElementRef) -> Result<Vec<PageItem>> {
        let mut result = Vec::new();
        let mut title: Option<String> = None;
        let mut url: Option<String> = None;
        let mut contents = Vec::new();

        // Function for finalizing a pending item into the result list. This is needed because
        // list items for pages within a chapter are separate items in the element tree.
        let mut finalize_pending_item =
            |title: &mut Option<String>, url: &mut Option<String>, contents: &mut Vec<PageItem>| {
                if let (Some(title_ref), Some(url_ref)) = (&title, &url) {
                    if contents.is_empty() {
                        result.push(PageItem::Link(PageLink {
                            title: title_ref.to_string(),
                            url: format!("{}/{}", path, url_ref),
                        }))
                    } else {
                        result.push(PageItem::Category(Box::new(PageCategory {
                            title: title_ref.to_string(),
                            url: Some(format!("{}/{}", path, url_ref)),
                            contents: contents.split_off(0),
                        })))
                    }
                    *title = None;
                    *url = None;
                }
            };

        // Traverse through the element tree and collect the page items
        for item in element.children() {
            match item.value() {
                Node::Element(element) => {
                    if element.name() == "li" {
                        for sub_element in item.children() {
                            match sub_element.value() {
                                Node::Element(element) => match element.name() {
                                    "a" => {
                                        if let Some(href) = element.attr("href") {
                                            // Finalize any pending items that need to be processed
                                            finalize_pending_item(
                                                &mut title,
                                                &mut url,
                                                &mut contents,
                                            );

                                            // Grab the URL and title from the element
                                            url = Some(href.to_string());
                                            for text in sub_element.children() {
                                                if let Node::Text(text) = text.value() {
                                                    title = Some(text.text.trim().to_string());
                                                }
                                            }
                                        }
                                    }
                                    "ol" => {
                                        // Found an ordered list, this is a collection of pages
                                        // within a chapter.
                                        contents = Self::collect_book_pages(
                                            path,
                                            ElementRef::wrap(sub_element)
                                                .ok_or_else(|| anyhow!("Expected an element"))?,
                                        )?;
                                    }
                                    _ => (),
                                },
                                _ => (),
                            }
                        }
                    }
                }
                _ => (),
            }
        }

        // Finalize the last item and return the result
        finalize_pending_item(&mut title, &mut url, &mut contents);
        Ok(result)
    }

    /// Wrapper to parse a CSS selector. The error type from `scraper` is incompatible with
    /// `anyhow` so we must translate it manually.
    fn selector(path: &str) -> Result<Selector> {
        match Selector::parse(path) {
            Ok(selector) => Ok(selector),
            Err(e) => Err(anyhow!("Could not parse selector '{}': {}", path, e)),
        }
    }
}
