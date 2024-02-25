mod qt;

use anyhow::Result;
use clap::Parser;
use docdelve::progress::ProgressEvent;
use std::io::Write;

#[derive(Parser)]
struct Cli {
    version: String,
    #[clap(short, long)]
    verbose: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    println!("Building Qt {} documentation", cli.version);
    qt::QtDocumentationGenerator::new(docdelve::container::ContainerEngine::Podman, &cli.version)?
        .build(|event| {
            match event {
                ProgressEvent::Output(msg) => {
                    if cli.verbose {
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
        })?;
    println!("\r\x1b[2KBuild completed");
    Ok(())
}
