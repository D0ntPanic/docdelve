use anyhow::Error;
use docdelve::chest::ChestListEntry;
use docdelve::container::{Container, ContainerEngine};
use docdelve::content::{ChestContents, ChestItem, Page};
use docdelve::progress::ProgressEvent;
use regex::Regex;

pub struct StandardLibraryDocumentationGenerator {
    container: Container,
    version: String,
}

impl StandardLibraryDocumentationGenerator {
    /// Create a Rust standard library documentation generator for the given version of Rust
    pub fn new(engine: ContainerEngine, version: &str) -> anyhow::Result<Self> {
        // Validate version string
        if !Regex::new(r"^[0-9]+\.[0-9]+\.[0-9]+$")
            .unwrap()
            .is_match(version)
        {
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
    pub fn build<F>(&mut self, mut progress: F) -> anyhow::Result<()>
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
        let mut contents = ChestContents::new(
            "Rust",
            "rust",
            None,
            &self.version,
            "index.html",
            None,
            None,
        );

        contents.items.push(ChestItem::Page(Box::new(Page {
            title: "Rust documentation".into(),
            url: "index.html".to_string(),
            contents: Vec::new(),
        })));

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
}
