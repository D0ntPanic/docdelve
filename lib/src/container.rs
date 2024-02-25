use crate::chest::Chest;
use crate::progress::ProgressEvent;
use anyhow::{Error, Result};
use if_chain::if_chain;
use regex::Regex;
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};

/// Default base image to use for containers
const IMAGE_BASE: &'static str = "ubuntu:22.04";

/// A single step for building a container image
enum ContainerBuildStep {
    Command(Vec<String>),
    WorkingDirectory(String),
    Environment(String, String),
}

/// The type of progress event to emit for a container build step
enum ContainerProgressType {
    AptInstall,
    GitClone(Option<String>),
    NinjaBuild(String),
    Generic(String),
}

enum ContainerCommandType {
    Basic,
    BuildObject,
    GetArchive,
}

enum ContainerCommandResult {
    None,
    Identifier(String),
    Data(Chest),
}

/// Selection of container engine to use
pub enum ContainerEngine {
    Podman,
    Docker,
}

/// Builds container images and allows extraction of artifacts from the image
pub struct Container {
    engine: ContainerEngine,
    base_image: String,
    steps: Vec<ContainerBuildStep>,
    step_progress: BTreeMap<usize, ContainerProgressType>,
    first_apt: bool,
    image: Option<String>,
}

impl Container {
    /// Create a new set of instructions for building a container
    pub fn new(engine: ContainerEngine) -> Self {
        Self {
            engine,
            base_image: IMAGE_BASE.to_string(),
            steps: vec![],
            step_progress: BTreeMap::new(),
            first_apt: true,
            image: None,
        }
    }

    /// Run a command inside the container
    pub fn command(&mut self, parts: &[&str]) {
        self.steps.push(ContainerBuildStep::Command(
            parts.into_iter().map(|x| String::from(*x)).collect(),
        ));
    }

    /// Install packages using `apt-get`
    pub fn apt_install(&mut self, packages: &[&str]) {
        if self.first_apt {
            // On the first apt-get, we need to download the package list first
            self.command(&["apt-get", "update", "-y"]);
            self.first_apt = false;
        }

        let mut cmd: Vec<&str> = vec!["apt-get", "install", "--no-install-recommends", "-y"];
        cmd.extend(packages);
        self.command(&cmd);

        // Monitor progress of the apt-get command during build
        self.step_progress
            .insert(self.steps.len() + 1, ContainerProgressType::AptInstall);
    }

    /// Set the current working directory for the container
    pub fn work_dir(&mut self, path: &str) {
        self.steps
            .push(ContainerBuildStep::WorkingDirectory(path.to_string()));
    }

    /// Set an environment variable for the container
    pub fn env(&mut self, name: &str, value: &str) {
        self.steps.push(ContainerBuildStep::Environment(
            name.to_string(),
            value.to_string(),
        ));
    }

    /// Monitor the progress of a git clone for the previously added command
    pub fn git_clone_progress(&mut self, name: &str) {
        self.step_progress.insert(
            self.steps.len() + 1,
            ContainerProgressType::GitClone(Some(name.into())),
        );
    }

    /// Monitor the progress of a git submodule checkout for the previously added command
    pub fn git_submodule_progress(&mut self) {
        self.step_progress
            .insert(self.steps.len() + 1, ContainerProgressType::GitClone(None));
    }

    /// Monitor the progress of a ninja build for the previously added command
    pub fn ninja_build_progress(&mut self, desc: &str) {
        self.step_progress.insert(
            self.steps.len() + 1,
            ContainerProgressType::NinjaBuild(desc.to_string()),
        );
    }

    /// Issue a generic progress event for the previously added command
    pub fn generic_progress(&mut self, desc: &str) {
        self.step_progress.insert(
            self.steps.len() + 1,
            ContainerProgressType::Generic(desc.to_string()),
        );
    }

    /// Generate a Dockerfile for the given steps
    fn dockerfile(&self) -> String {
        let mut result = String::new();
        result.push_str(&format!("FROM {}\n", self.base_image));
        for step in &self.steps {
            match step {
                ContainerBuildStep::Command(parts) => {
                    result.push_str("RUN [");
                    let mut first = true;
                    for part in parts {
                        if !first {
                            result.push_str(", ");
                        }
                        first = false;
                        result.push_str("\"");
                        result.push_str(&part.escape_default().to_string());
                        result.push_str("\"");
                    }
                    result.push_str("]\n");
                }
                ContainerBuildStep::WorkingDirectory(path) => {
                    result.push_str(&format!("WORKDIR {}\n", path));
                }
                ContainerBuildStep::Environment(name, value) => {
                    result.push_str(&format!("ENV {}={}\n", name, value));
                }
            }
        }
        result
    }

    /// Execute a command using the specified container engine
    fn exec_command<F>(
        &self,
        progress: &mut F,
        args: &[&str],
        stdin_contents: Vec<u8>,
        cmd_type: ContainerCommandType,
    ) -> Result<ContainerCommandResult>
    where
        F: FnMut(ProgressEvent),
    {
        // Spawn the container process with the given arguments
        let engine_executable = match &self.engine {
            ContainerEngine::Podman => "podman",
            ContainerEngine::Docker => "docker",
        };
        let mut cmd = Command::new(engine_executable)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        // Send the requested input to stdin
        let mut stdin = cmd
            .stdin
            .take()
            .ok_or(Error::msg("Failed to access stidn"))?;
        std::thread::spawn(move || stdin.write_all(&stdin_contents));

        // Grab stdout from the container process
        let mut stdout = BufReader::new(
            cmd.stdout
                .take()
                .ok_or(Error::msg("Failed to capture stdout"))?,
        );

        if matches!(cmd_type, ContainerCommandType::GetArchive) {
            // Command is to get an archive from the container. Read the archive from stdout and
            // decode it as a tar archive.
            let mut result = Chest::new();
            let mut tar = tar::Archive::new(&mut stdout);
            for entry in tar.entries()? {
                let mut entry = entry?;
                let mut contents = Vec::new();
                entry.read_to_end(&mut contents)?;
                if entry.header().entry_type() == tar::EntryType::Regular {
                    // Get the path to the file in the archive
                    let name = entry
                        .path()?
                        .to_str()
                        .ok_or_else(|| Error::msg("Bad path in tar archive"))?
                        .to_string();

                    // Strip off the first part of the path so that the archive contains
                    // only the contents of the path, not the path itself.
                    let name = if let Some(index) = name.find('/') {
                        &name[index + 1..]
                    } else {
                        &name
                    };

                    result.write(&name, &contents)?;
                }
            }
            return Ok(ContainerCommandResult::Data(result));
        }

        // Set up progress monitoring state and regular expressions
        let mut step = 0;
        let mut expected_build_total = None;
        let mut last_line = None;
        let step_regex = Regex::new(r"^STEP ([0-9]+)/[0-9]+:")?;
        let apt_download_regex = Regex::new(r"^Get:[0-9]+ [^ ]+ [^ ]+ [^ ]+ ([^ ]+)")?;
        let apt_install_regex = Regex::new(r"^Setting up ([^: ]+)")?;
        let clone_regex = Regex::new(r"^Cloning into '(.*)'\.\.\.$")?;
        let ninja_regex = Regex::new(r"^\[([0-9]+)/([0-9]+)]")?;

        // Watch for output
        for line in stdout.lines() {
            // Report raw output to caller
            let line = line?;
            progress(ProgressEvent::Output(line.clone()));

            // Check for new build step
            if let Some(captures) = step_regex.captures(&line) {
                if let Some(capture) = captures.get(1) {
                    if let Ok(n) = capture.as_str().parse::<usize>() {
                        step = n;
                        expected_build_total = None;

                        // For generic progress types, send progress event when step starts
                        match self.step_progress.get(&step) {
                            Some(ContainerProgressType::Generic(desc)) => {
                                progress(ProgressEvent::Action(desc.to_string()));
                            }
                            _ => (),
                        }
                    }
                }
            }

            // Check for a progress type on this step
            match self.step_progress.get(&step) {
                Some(ContainerProgressType::AptInstall) => {
                    // Check for an apt package being downloaded
                    if let Some(captures) = apt_download_regex.captures(&line) {
                        if let Some(capture) = captures.get(1) {
                            progress(ProgressEvent::DownloadPackage(capture.as_str().into()));
                        }
                    }

                    // Check for an apt package being installed
                    if let Some(captures) = apt_install_regex.captures(&line) {
                        if let Some(capture) = captures.get(1) {
                            progress(ProgressEvent::InstallPackage(capture.as_str().into()));
                        }
                    }
                }
                Some(ContainerProgressType::GitClone(fixed_name)) => {
                    if let Some(captures) = clone_regex.captures(&line) {
                        // Found a git clone
                        if let Some(name) = fixed_name {
                            // Clone target doesn't mean anything but a name was given by the
                            // container creator.
                            progress(ProgressEvent::DownloadSource(name.clone()));
                        } else if let Some(capture) = captures.get(1) {
                            // In a submodule step, clone target will be used as the name
                            progress(ProgressEvent::DownloadSource(capture.as_str().into()));
                        }
                    }
                }
                Some(ContainerProgressType::NinjaBuild(desc)) => {
                    // Watch for ninja build steps
                    if_chain! {
                        if let Some(captures) = ninja_regex.captures(&line);
                        if let Some(build_step) = captures.get(1);
                        if let Some(total) = captures.get(2);
                        if let Ok(build_step) = build_step.as_str().parse::<u64>();
                        if let Ok(total) = total.as_str().parse::<u64>();
                        then {
                            if let Some(expected_total) = expected_build_total {
                                // If there is a build already detected, ensure it is the
                                // main build, not a recursively spawned build.
                                if total == expected_total {
                                    progress(ProgressEvent::Build(
                                        desc.into(),
                                        build_step - 1,
                                        total,
                                    ));
                                    if build_step == total {
                                        expected_build_total = None;
                                    }
                                }
                            } else {
                                // Keep track of the expected total for the build, to prevent
                                // recursive builds from messing with the overall progress.
                                expected_build_total = Some(total);
                                progress(ProgressEvent::Build(
                                    desc.into(),
                                    build_step - 1,
                                    total,
                                ));
                            }
                        }
                    }
                }
                _ => (),
            }

            // Keep track of the last line of output, as this will eventually be the
            // image or container identifier
            last_line = Some(line.clone());
        }

        // Wait for the process to exit and check for success
        if !cmd.wait()?.success() {
            return Err(Error::msg("Command failed"));
        }

        match cmd_type {
            ContainerCommandType::Basic => {
                // Container command has no output needed by caller
                Ok(ContainerCommandResult::None)
            }
            ContainerCommandType::BuildObject => {
                // This command should have an identifier, check for it and return it
                // if it is found.
                if let Some(last_line) = last_line {
                    if Regex::new(r"^[0-9a-fA-F]+$")?.is_match(&last_line) {
                        return Ok(ContainerCommandResult::Identifier(last_line));
                    }
                }
                Err(Error::msg("Output identifier was not given"))
            }
            ContainerCommandType::GetArchive => unreachable!(),
        }
    }

    /// Build a container image using the steps provided. Progress will be reported as a
    /// stream of events passed to `progress`.
    pub fn build<F>(&mut self, progress: &mut F) -> Result<()>
    where
        F: FnMut(ProgressEvent),
    {
        // Write the container commands to a file
        let dockerfile = self.dockerfile();

        // Build the container with the given commands and grab the image identifier
        if let ContainerCommandResult::Identifier(image) = self.exec_command(
            progress,
            &["build", "-f", "-"],
            dockerfile.as_bytes().to_vec(),
            ContainerCommandType::BuildObject,
        )? {
            self.image = Some(image);
        } else {
            return Err(Error::msg("Failed to obtain image identifier"));
        }

        Ok(())
    }

    /// Perform actions while holding onto a container. The container will be removed once
    /// the actions are complete or have failed.
    fn with_container<F, T>(&self, container: &str, func: F) -> Result<T>
    where
        F: FnOnce() -> Result<T>,
    {
        let result = func();

        // Delete the container after use. If this fails we can't do anything about it.
        let _ = self.exec_command(
            &mut |_| {},
            &["container", "rm", &container],
            Vec::new(),
            ContainerCommandType::Basic,
        );

        result
    }

    /// Get a tar archive of a path inside the built image. The image must first be built with `build`.
    pub fn get_archive(&self, path: &str) -> Result<Chest> {
        // Get the image identifier
        let image = match &self.image {
            Some(image) => image,
            None => return Err(Error::msg("No image has been built")),
        };

        // Create a container using the image. We can't copy out any contents without
        // having a container.
        let container = match self.exec_command(
            &mut |_| {},
            &["container", "create", image],
            Vec::new(),
            ContainerCommandType::BuildObject,
        )? {
            ContainerCommandResult::Identifier(container) => container,
            _ => return Err(Error::msg("Failed to obtain container identifier")),
        };

        // Copy the contents out of the container. The container will be removed when
        // the command is complete or has failed.
        let archive = self.with_container(&container, || {
            match self.exec_command(
                &mut |_| {},
                &["container", "cp", &format!("{}:{}", container, path), "-"],
                Vec::new(),
                ContainerCommandType::GetArchive,
            )? {
                ContainerCommandResult::Data(archive) => Ok(archive),
                _ => Err(Error::msg("Failed to obtain archive")),
            }
        })?;

        Ok(archive)
    }
}
