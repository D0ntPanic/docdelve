mod qt;

use anyhow::Result;
use clap::Parser;
use docdelve::progress::default_terminal_progress_event_handler;

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
        .build(default_terminal_progress_event_handler(cli.verbose))?;
    println!("\r\x1b[2KBuild completed");
    Ok(())
}
