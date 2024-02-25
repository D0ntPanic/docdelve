use std::io::Write;

pub enum ProgressEvent {
    Output(String),
    DownloadPackage(String),
    InstallPackage(String),
    DownloadSource(String),
    Build(String, u64, u64),
    Action(String),
    CompressChest(u64, u64),
    ExtractChest(u64, u64),
}

pub fn default_terminal_progress_event_handler(verbose: bool) -> Box<dyn Fn(ProgressEvent)> {
    Box::new(move |event| {
        match event {
            ProgressEvent::Output(msg) => {
                if verbose {
                    println!("\r\x1b[2K{}", msg);
                }
            }
            ProgressEvent::DownloadPackage(package) => {
                print!("\r\x1b[2KDownloading package {}...", package)
            }
            ProgressEvent::InstallPackage(package) => {
                print!("\r\x1b[2KInstalling package {}...", package)
            }
            ProgressEvent::DownloadSource(repo) => {
                print!("\r\x1b[2KDownloading source {}...", repo)
            }
            ProgressEvent::Build(desc, done, total) => {
                print!("\r\x1b[2KBuilding {} ({}/{})...", desc, done, total)
            }
            ProgressEvent::Action(desc) => {
                print!("\r\x1b[2K{}...", desc)
            }
            ProgressEvent::CompressChest(done, total) => {
                print!("\r\x1b[2KCompressing chest ({}%)...", (done * 100) / total)
            }
            ProgressEvent::ExtractChest(done, total) => {
                print!("\r\x1b[2KExtracting chest ({}%)...", (done * 100) / total)
            }
        }
        let _ = std::io::stdout().flush();
    })
}
