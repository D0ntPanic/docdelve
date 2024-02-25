use crate::progress::ProgressEvent;
use anyhow::{anyhow, Error, Result};
use diffy::Patch;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::FileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Directory listing for a directory within a chest
struct ChestDirectory {
    contents: BTreeMap<String, ChestDirectoryEntry>,
}

/// Directory entry for a name in the chest
enum ChestDirectoryEntry {
    File(ChestFile),
    Directory(Box<ChestDirectory>),
}

/// Tracks a single file's contents. May either be in memory contents or stored in a zip file
/// on disk. If stored in a zip file, the path is the same as the path within the chest.
enum ChestFile {
    InMemoryFile(Vec<u8>),
    ZipBackedFile,
}

/// Directory listing entry for querying the contents of a chest
pub enum ChestListEntry {
    File(String),
    Directory(String),
}

/// Tracks a bundle of files called a chest. This may either be stored in memory or backed by a
/// zip file on disk.
pub struct Chest {
    root: ChestDirectory,
    backing_zip: Option<RefCell<ZipArchive<BufReader<File>>>>,
    path: Option<PathBuf>,
}

impl Chest {
    /// Create a new, empty chest
    pub fn new() -> Self {
        Self {
            root: ChestDirectory {
                contents: BTreeMap::new(),
            },
            backing_zip: None,
            path: None,
        }
    }

    pub fn open(path: &Path) -> Result<Self> {
        // Open the chest file as a zip archive
        let chest = BufReader::new(File::open(path)?);
        let zip = ZipArchive::new(chest)?;

        // Create the chest structure. Don't place the zip file into the structure yet
        // to avoid needing to borrow it.
        let mut result = Self {
            root: ChestDirectory {
                contents: BTreeMap::new(),
            },
            backing_zip: None,
            path: Some(path.to_path_buf()),
        };

        // Iterate over the entries in the zip archive
        for name in zip.file_names() {
            if name.ends_with("/") {
                // Skip directories
                continue;
            }

            // Write an entry for each file to declare that it is found in the zip archive.
            // Intermediate directories will be created as needed.
            result.write_file_entry(name, |entry| {
                *entry = ChestDirectoryEntry::File(ChestFile::ZipBackedFile);
                Ok(())
            })?;
        }

        // Place the zip file into the structure so that files can be read later
        result.backing_zip = Some(RefCell::new(zip));
        Ok(result)
    }

    /// Check a component of a path to see if it is valid
    fn validate_name(name: &str) -> Result<()> {
        if name.is_empty() {
            return Err(Error::msg("Path components cannot be empty"));
        }
        for ch in name.chars() {
            // Don't allow any characters that will be invalid in a file name
            // for any major OS, not just the current one.
            if ch.is_ascii_control()
                || ch == '/'
                || ch == '\\'
                || ch == '<'
                || ch == '>'
                || ch == '"'
                || ch == ':'
                || ch == '|'
                || ch == '?'
                || ch == '*'
            {
                return Err(Error::msg("Path component has invalid characters"));
            }
        }
        Ok(())
    }

    /// Read a directory entry at the given path. The given function will be called with
    /// a reference to the directory entry. The entry can be either a file or a subdirectory.
    fn read_entry<F, T>(&self, path: &str, func: F) -> Result<T>
    where
        F: FnOnce(&ChestDirectoryEntry) -> Result<T>,
    {
        // Split the path into its components
        let mut parts: Vec<&str> = path.split('/').collect();
        let filename = match parts.pop() {
            Some(filename) => filename,
            None => return Err(Error::msg("Path cannot be empty")),
        };
        if parts.len() > 0 && parts[0].is_empty() {
            // Remove leading slash
            parts.remove(0);
        }

        // Traverse into the directory that should contain the file
        let mut current = &self.root;
        for part in parts {
            let entry = current.contents.get(part);
            match entry {
                Some(ChestDirectoryEntry::Directory(directory)) => {
                    // Follow into directory
                    current = directory;
                }
                _ => {
                    return Err(Error::msg("Path not found"));
                }
            }
        }

        // Get the directory entry, or create a new file in its place if it doesn't exist
        let entry = current.contents.get(filename);
        match entry {
            Some(entry) => {
                // Call the callback to read the directory entry
                func(entry)
            }
            None => Err(Error::msg("Path not found")),
        }
    }

    /// Read a file at the given path. The given function will be called with
    /// a reference to the file entry.
    fn read_file_entry<F, T>(&self, path: &str, func: F) -> Result<T>
    where
        F: FnOnce(&ChestFile) -> Result<T>,
    {
        self.read_entry(path, |entry| {
            match entry {
                ChestDirectoryEntry::File(file) => {
                    // Call the callback to read the file
                    func(file)
                }
                _ => Err(Error::msg("File not found")),
            }
        })
    }

    /// Write a file at the given path. The given function will be called with a mutable
    /// reference to the directory entry, which may be a newly created empty file. The
    /// file can be replaced with the desired contents.
    fn write_file_entry<F, T>(&mut self, path: &str, func: F) -> Result<T>
    where
        F: FnOnce(&mut ChestDirectoryEntry) -> Result<T>,
    {
        // Split the path into its components
        let mut parts: Vec<&str> = path.split('/').collect();
        let filename = match parts.pop() {
            Some(filename) => filename,
            None => return Err(Error::msg("Path cannot be empty")),
        };
        if parts.len() > 0 && parts[0].is_empty() {
            // Remove leading slash
            parts.remove(0);
        }

        // Traverse into the directory that will contain the file
        let mut current = &mut self.root;
        for part in parts {
            // Validate each path component's name
            Self::validate_name(part)?;

            // Get the directory entry, or create a new directory in its place if it doesn't exist
            let entry = current.contents.entry(part.to_string()).or_insert_with(|| {
                ChestDirectoryEntry::Directory(Box::new(ChestDirectory {
                    contents: BTreeMap::new(),
                }))
            });

            match entry {
                ChestDirectoryEntry::Directory(directory) => {
                    // Follow into directory
                    current = directory;
                }
                ChestDirectoryEntry::File(_) => {
                    return Err(Error::msg(
                        "Cannot create directory because a file already exists there",
                    ));
                }
            }
        }

        // Get the directory entry, or create a new file in its place if it doesn't exist
        Self::validate_name(filename)?;
        let entry = current
            .contents
            .entry(filename.to_string())
            .or_insert_with(|| ChestDirectoryEntry::File(ChestFile::InMemoryFile(Vec::new())));

        match entry {
            ChestDirectoryEntry::File(_) => {
                // Call the callback to write the new file contents
                func(entry)
            }
            ChestDirectoryEntry::Directory(_) => Err(Error::msg(
                "Cannot write file because a directory already exists there",
            )),
        }
    }

    /// Determines if a chest contains a path
    pub fn contains(&self, filename: &str) -> bool {
        let mut result = false;
        if matches!(
            self.read_file_entry(filename, |_| {
                result = true;
                Ok(())
            }),
            Ok(())
        ) {
            result
        } else {
            false
        }
    }

    /// Read a file from the chest
    pub fn read(&self, mut path: &str) -> Result<Vec<u8>> {
        self.read_file_entry(path, |file| match file {
            ChestFile::InMemoryFile(contents) => Ok(contents.clone()),
            ChestFile::ZipBackedFile => match &self.backing_zip {
                Some(zip) => {
                    // Extract the file from the zip archive
                    let mut contents = Vec::new();
                    if path.starts_with("/") {
                        path = &path[1..];
                    }
                    zip.borrow_mut().by_name(path)?.read_to_end(&mut contents)?;
                    Ok(contents)
                }
                None => Err(Error::msg(
                    "File is backed by a zip file, but no backing zip file is present",
                )),
            },
        })
    }

    /// Write a file to the chest. If the file already exists, it will be overwritten. If the
    /// directories that are referenced by the path do not exist, they will be created.
    pub fn write(&mut self, path: &str, data: &[u8]) -> Result<()> {
        self.write_file_entry(path, |entry| {
            *entry = ChestDirectoryEntry::File(ChestFile::InMemoryFile(data.to_vec()));
            Ok(())
        })
    }

    /// Removes a file or directory at the given path. If deleting a directory, all files
    /// within the directory will also be deleted.
    pub fn remove(&mut self, path: &str) -> Result<()> {
        // Split the path into its components
        let mut parts: Vec<&str> = path.split('/').collect();
        let filename = match parts.pop() {
            Some(filename) => filename,
            None => return Err(Error::msg("Path cannot be empty")),
        };
        if parts.len() > 0 && parts[0].is_empty() {
            // Remove leading slash
            parts.remove(0);
        }

        // Traverse into the directory that will contain the path
        let mut current = &mut self.root;
        for part in parts {
            // Validate each path component's name
            Self::validate_name(part)?;

            // Get the directory entry, or create a new directory in its place if it doesn't exist
            let entry = current.contents.entry(part.to_string()).or_insert_with(|| {
                ChestDirectoryEntry::Directory(Box::new(ChestDirectory {
                    contents: BTreeMap::new(),
                }))
            });

            match entry {
                ChestDirectoryEntry::Directory(directory) => {
                    // Follow into directory
                    current = directory;
                }
                ChestDirectoryEntry::File(_) => {
                    return Err(Error::msg("Path not found"));
                }
            }
        }

        // Remove the directory entry if it exists
        Self::validate_name(filename)?;
        if current.contents.contains_key(filename) {
            current.contents.remove(filename);
            Ok(())
        } else {
            Err(Error::msg("Path not found"))
        }
    }

    /// Get a directory listing for a directory
    pub fn list_dir(&self, path: &str) -> Result<Vec<ChestListEntry>> {
        // Split the path into its components
        let mut parts: Vec<&str> = path.split('/').collect();
        if parts.len() > 0 && parts[0].is_empty() {
            // Remove leading slash
            parts.remove(0);
        }
        if let Some(last) = parts.last() {
            if last.is_empty() {
                // Remove trailing slash
                parts.pop();
            }
        }

        // Traverse into the directory that is being listed
        let mut current = &self.root;
        for part in parts {
            let entry = current.contents.get(part);
            match entry {
                Some(ChestDirectoryEntry::Directory(directory)) => {
                    // Follow into directory
                    current = directory;
                }
                _ => {
                    return Err(Error::msg("Path not found"));
                }
            }
        }

        // Obtain the directory listing for the directory
        let mut result = Vec::new();
        for (name, entry) in current.contents.iter() {
            match entry {
                ChestDirectoryEntry::Directory(_) => {
                    result.push(ChestListEntry::Directory(name.clone()));
                }
                ChestDirectoryEntry::File(_) => {
                    result.push(ChestListEntry::File(name.clone()));
                }
            }
        }

        Ok(result)
    }

    /// Gets the total size of all files in the chest.
    pub fn total_size(&self) -> u64 {
        let mut result = 0;

        // Traverse through the entire chest's directory structure
        let mut dir_queue = vec![(None, &self.root)];
        while !dir_queue.is_empty() {
            // Get the next directory to work on
            let (dir_path, dir) = dir_queue.pop().unwrap();

            let path = if let Some(dir_path) = dir_path {
                // Non-root directory
                format!("{}/", dir_path)
            } else {
                // Root directory
                String::new()
            };

            // Process each entry in the directory
            for (name, entry) in dir.contents.iter() {
                match entry {
                    ChestDirectoryEntry::Directory(subdir) => {
                        // Found a subdirectory. Add it to the queue for later.
                        dir_queue.push((Some(format!("{}{}", path, name)), subdir));
                    }
                    ChestDirectoryEntry::File(ChestFile::InMemoryFile(contents)) => {
                        // Found an in memory file. Add it to the total.
                        result += contents.len() as u64;
                    }
                    ChestDirectoryEntry::File(ChestFile::ZipBackedFile) => {
                        if let Some(existing_zip) = &self.backing_zip {
                            let file_path = format!("{}{}", path, name);
                            result += existing_zip
                                .borrow_mut()
                                .by_name(&file_path)
                                .map(|file| file.size())
                                .unwrap_or(0);
                        }
                    }
                }
            }
        }

        result
    }

    /// Saves the chest contents to a new zip archive
    pub fn save<F>(&mut self, path: &Path, mut progress: F) -> Result<()>
    where
        F: FnMut(ProgressEvent),
    {
        // Create the zip archive. Use zstd compression as it is much faster than deflate.
        let chest = BufWriter::new(File::create(path)?);
        let mut zip = ZipWriter::new(chest);
        let options = FileOptions::default()
            .compression_method(CompressionMethod::Zstd)
            .compression_level(Some(7));

        // Traverse through the entire chest's directory structure
        let mut dir_queue: Vec<(Option<String>, &ChestDirectory)> = vec![(None, &self.root)];
        let mut done = 0;
        let total = self.total_size();
        while !dir_queue.is_empty() {
            // Get the next directory to work on
            let (dir_path, dir) = dir_queue.pop().unwrap();

            let path = if let Some(dir_path) = dir_path {
                // Non-root directory. Add the directory path to the zip archive
                zip.add_directory(dir_path.to_string(), options)?;
                format!("{}/", dir_path)
            } else {
                // Root directory
                String::new()
            };

            // Process each entry in the directory
            for (name, entry) in dir.contents.iter() {
                match entry {
                    ChestDirectoryEntry::Directory(subdir) => {
                        // Found a subdirectory. Add it to the queue for later.
                        dir_queue.push((Some(format!("{}{}", path, name)), subdir));
                    }
                    ChestDirectoryEntry::File(ChestFile::InMemoryFile(contents)) => {
                        // Found an in memory file. Add it to the zip archive.
                        zip.start_file(format!("{}{}", path, name), options)?;
                        zip.write_all(contents)?;

                        done += contents.len() as u64;
                        progress(ProgressEvent::CompressChest(done, total));
                    }
                    ChestDirectoryEntry::File(ChestFile::ZipBackedFile) => {
                        // Found a zip backed file. First read the file contents from the existing
                        // zip archive.
                        let mut contents = Vec::new();
                        let file_path = format!("{}{}", path, name);
                        let size = if let Some(existing_zip) = &self.backing_zip {
                            let mut zip = existing_zip.borrow_mut();
                            let mut file = zip.by_name(&file_path)?;
                            let size = file.size();
                            file.read_to_end(&mut contents)?;
                            size
                        } else {
                            return Err(Error::msg(
                                "File is backed by a zip file, but no backing zip file is present",
                            ));
                        };

                        // Write the contents to the new zip archive
                        zip.start_file(file_path, options)?;
                        zip.write_all(&contents)?;

                        done += size;
                        progress(ProgressEvent::CompressChest(done, total));
                    }
                }
            }
        }

        // Finalize the zip archive
        zip.finish()?;

        self.path = Some(path.to_path_buf());
        Ok(())
    }

    /// Extracts the chest contents to a directory
    pub fn extract<F>(&self, path: &Path, mut progress: F) -> Result<()>
    where
        F: FnMut(ProgressEvent),
    {
        // Traverse through the entire chest's directory structure
        let mut dir_queue: Vec<(Option<PathBuf>, Option<String>, &ChestDirectory)> =
            vec![(None, None, &self.root)];
        let mut done = 0;
        let total = self.total_size();
        while !dir_queue.is_empty() {
            // Get the next directory to work on
            let (target_path, src_path, dir) = dir_queue.pop().unwrap();

            let target_path = if let Some(target_path) = target_path {
                target_path
            } else {
                path.to_owned()
            };

            let src_path = if let Some(src_path) = src_path {
                format!("{}/", src_path)
            } else {
                String::new()
            };

            // Ensure target directory exists
            std::fs::create_dir_all(&target_path)?;

            // Process each entry in the directory
            for (name, entry) in dir.contents.iter() {
                match entry {
                    ChestDirectoryEntry::Directory(subdir) => {
                        // Found a subdirectory. Add it to the queue for later.
                        let mut target_path = target_path.clone();
                        target_path.push(name);
                        dir_queue.push((
                            Some(target_path),
                            Some(format!("{}{}", src_path, name)),
                            subdir,
                        ));
                    }
                    ChestDirectoryEntry::File(ChestFile::InMemoryFile(contents)) => {
                        // Found an in memory file. Write it to the directory.
                        let mut target_path = target_path.clone();
                        target_path.push(name);
                        std::fs::write(&target_path, contents)?;

                        done += contents.len() as u64;
                        progress(ProgressEvent::ExtractChest(done, total));
                    }
                    ChestDirectoryEntry::File(ChestFile::ZipBackedFile) => {
                        // Found a zip backed file. First read the file contents from the existing
                        // zip archive.
                        let mut contents = Vec::new();
                        let mut target_path = target_path.clone();
                        target_path.push(name);
                        let src_path = format!("{}{}", src_path, name);
                        let size = if let Some(existing_zip) = &self.backing_zip {
                            let mut zip = existing_zip.borrow_mut();
                            let mut file = zip.by_name(&src_path)?;
                            let size = file.size();
                            file.read_to_end(&mut contents)?;
                            size
                        } else {
                            return Err(Error::msg(
                                "File is backed by a zip file, but no backing zip file is present",
                            ));
                        };

                        // Write the contents to the directory
                        std::fs::write(&target_path, &contents)?;

                        done += size;
                        progress(ProgressEvent::ExtractChest(done, total));
                    }
                }
            }
        }

        Ok(())
    }

    /// Gets the on disk path of the chest, if it is on disk.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_ref().map(|path| path.as_path())
    }

    /// Closes the chest and deletes it from disk.
    pub fn delete(mut self) -> Result<()> {
        if let Some(path) = self.path {
            // Need to close the file first or the deletion will fail on Windows.
            self.backing_zip.take();

            Ok(std::fs::remove_file(path)?)
        } else {
            Err(anyhow!("Chest is not on disk"))
        }
    }

    /// Finds all occurrences of a filename in the chest and returns a list of paths to
    /// those files.
    pub fn find_all(&self, filename: &str) -> Vec<String> {
        // Traverse through the entire chest's directory structure
        let mut dir_queue: Vec<(Option<String>, &ChestDirectory)> = vec![(None, &self.root)];
        let mut result = Vec::new();
        while !dir_queue.is_empty() {
            // Get the next directory to work on
            let (src_path, dir) = dir_queue.pop().unwrap();
            let src_path = if let Some(src_path) = src_path {
                format!("{}/", src_path)
            } else {
                String::new()
            };

            // Process each entry in the directory
            for (name, entry) in dir.contents.iter() {
                match entry {
                    ChestDirectoryEntry::Directory(subdir) => {
                        // Found a subdirectory. Add it to the queue for later.
                        dir_queue.push((Some(format!("{}{}", src_path, name)), subdir));
                    }
                    ChestDirectoryEntry::File(_) => {
                        if name == filename {
                            result.push(format!("{}{}", src_path, name));
                        }
                    }
                }
            }
        }
        result
    }

    /// Applies a patch to a file within the chest.
    pub fn patch(&mut self, path: &str, patch: &Patch<str>) -> Result<()> {
        let contents = &self.read(path)?;
        let string = std::str::from_utf8(&contents)?;
        let result = diffy::apply(&string, patch)?;
        self.write(path, result.as_bytes())
    }

    /// Transforms a chest path. If `pattern` starts with a slash, matches the entire path
    /// exactly and replaces it with `replacement` if it matches. If `pattern` does not start
    /// with a slash, matches the trailing subcomponents of the path. If `replacement` starts
    /// with a slash, the new path is replaced entirely on a match. If `replacement` does not
    /// start with a slash, only the matched components of the path are replaced.
    pub fn transform_path(path: &str, pattern: &str, replacement: &str) -> Option<String> {
        if path == pattern {
            if replacement.starts_with('/') {
                return Some((&replacement[1..]).to_string());
            } else {
                return Some(replacement.to_string());
            }
        } else if !pattern.starts_with('/') {
            if let Some(prefix) = path.strip_suffix(&format!("/{}", pattern)) {
                if replacement.starts_with('/') {
                    return Some((&replacement[1..]).to_string());
                } else {
                    return Some(format!("{}/{}", prefix, replacement));
                }
            }
        }
        None
    }
}
