mod stdlib;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use docdelve::progress::default_terminal_progress_event_handler;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    #[clap(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    Std(StandardLibraryCommand),
    Crate(CrateCommand),
}

#[derive(Args)]
struct StandardLibraryCommand {
    version: String,
}

#[derive(Args)]
struct CrateCommand {
    name: String,
    version: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Std(std) => {
            println!(
                "Building standard library documentation for version {}",
                std.version
            );

            stdlib::StandardLibraryDocumentationGenerator::new(
                docdelve::container::ContainerEngine::Podman,
                &std.version,
            )?
            .build(default_terminal_progress_event_handler(cli.verbose))?;
            println!("\r\x1b[2KBuild completed");
        }
        Command::Crate(_) => {
            println!("Crate documentation generation is not yet implemented");
        }
    }
    Ok(())
}
