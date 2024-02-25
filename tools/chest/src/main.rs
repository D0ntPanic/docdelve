use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use docdelve::chest::Chest;
use docdelve::content::{ChestContents, ChestItem, IndexedChestItemData, ObjectType, PageItem};
use docdelve::db::{Database, SearchParameters};
use docdelve::progress::ProgressEvent;
use std::path::PathBuf;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Extract(ExtractArgs),
    List(ListArgs),
    Install(InstallArgs),
    Search(SearchArgs),
}

#[derive(Args)]
struct ExtractArgs {
    chest: PathBuf,
    target: PathBuf,
}

#[derive(Args)]
struct ListArgs {
    chest: PathBuf,
}

#[derive(Args)]
struct InstallArgs {
    chest: PathBuf,
}

#[derive(Args)]
struct SearchArgs {
    query: String,
}

pub fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Extract(extract) => {
            let chest = Chest::open(&extract.chest)?;
            chest.extract(&extract.target, |event| match event {
                ProgressEvent::ExtractChest(done, total) => {
                    print!("\r\x1b[2KExtracting chest ({}%)...", (done * 100) / total)
                }
                _ => (),
            })?;
            println!("\r\x1b[2KExtract completed");
        }
        Commands::List(list) => {
            let chest = Chest::open(&list.chest)?;
            let contents = ChestContents::read_from_chest(&chest)?;
            dump_contents(&contents);
        }
        Commands::Install(install) => {
            let chest = Chest::open(&install.chest)?;
            let mut db = Database::load()?;
            db.install(&chest)?;
        }
        Commands::Search(search) => {
            let db = Database::load()?;

            let start = std::time::Instant::now();
            let results = db.search(None, &search.query, SearchParameters::default());
            let t = std::time::Instant::now().duration_since(start);
            println!("Search completed in {}ms", t.as_millis());

            for result in results {
                println!(
                    "{} {}:{} ({})",
                    result.path.identifier,
                    db.tag_for_identifier(&result.path.identifier)
                        .unwrap_or_default(),
                    result.path.chest_path,
                    result.score
                );
                for item in db.items_at_path(&result.path) {
                    if let IndexedChestItemData::Object(obj) = &item.data {
                        if let Some(decl) = &obj.info.declaration {
                            println!("  {}", decl);
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

fn dump_contents(contents: &ChestContents) {
    println!(
        "{} version {}, tag '{}', start page '{}'",
        contents.info.name,
        contents.info.version,
        contents.info.category_tag,
        contents.info.start_url
    );
    if let Some(extension_module) = &contents.info.extension_module {
        println!("Extension module {}", extension_module);
    }

    println!();
    println!("Items in chest:");

    dump_items(&contents.items, 0);
}

fn dump_items(items: &Vec<ChestItem>, indent_count: usize) {
    let indent = "  ".repeat(indent_count);
    for item in items {
        match item {
            ChestItem::Module(module) => {
                if let Some(url) = &module.info.url {
                    println!(
                        "{}Module {} ({}) -> {}",
                        indent, module.info.name, module.info.full_name, url
                    );
                } else {
                    println!(
                        "{}Module {} ({})",
                        indent, module.info.name, module.info.full_name
                    );
                }
                dump_items(&module.contents, indent_count + 1);
            }
            ChestItem::Group(group) => {
                if let Some(url) = &group.info.url {
                    println!("{}Group {} -> {}", indent, group.info.name, url);
                } else {
                    println!("{}Group {}", indent, group.info.name);
                }
                dump_items(&group.contents, indent_count + 1);
            }
            ChestItem::Page(page) => {
                println!("{}Page: {} -> {}", indent, page.title, page.url);
                dump_page(&page.contents, indent_count + 1);
            }
            ChestItem::Object(obj) => {
                let obj_type = match obj.info.object_type {
                    ObjectType::Class => "Class",
                    ObjectType::Struct => "Struct",
                    ObjectType::Union => "Union",
                    ObjectType::Object => "Object",
                    ObjectType::Enum => "Enum",
                    ObjectType::Value => "Value",
                    ObjectType::Variant => "Variant",
                    ObjectType::Trait => "Trait",
                    ObjectType::TraitImplementation => "TraitImplementation",
                    ObjectType::Interface => "Interface",
                    ObjectType::Function => "Function",
                    ObjectType::Method => "Method",
                    ObjectType::Variable => "Variable",
                    ObjectType::Member => "Member",
                    ObjectType::Field => "Field",
                    ObjectType::Constant => "Constant",
                    ObjectType::Property => "Property",
                    ObjectType::Typedef => "Typedef",
                    ObjectType::Namespace => "Namespace",
                };
                if let Some(url) = &obj.info.url {
                    if let Some(decl) = &obj.info.declaration {
                        println!(
                            "{}{} {} ({}) {{ {} }} -> {}",
                            indent, obj_type, obj.info.name, obj.info.full_name, decl, url
                        );
                    } else {
                        println!(
                            "{}{} {} ({}) -> {}",
                            indent, obj_type, obj.info.name, obj.info.full_name, url
                        );
                    }
                } else {
                    if let Some(decl) = &obj.info.declaration {
                        println!(
                            "{}{} {} ({}) {{ {} }}",
                            indent, obj_type, obj.info.name, obj.info.full_name, decl
                        );
                    } else {
                        println!(
                            "{}{} {} ({})",
                            indent, obj_type, obj.info.name, obj.info.full_name
                        );
                    }
                }
                for base in &obj.info.bases {
                    println!(
                        "{}  Base: {}",
                        indent,
                        base.elements
                            .iter()
                            .map(|element| element.name.as_str())
                            .collect::<Vec<_>>()
                            .join(".")
                    );
                }
                dump_items(&obj.contents, indent_count + 1);
            }
        }
    }
}

fn dump_page(items: &Vec<PageItem>, indent_count: usize) {
    let indent = "  ".repeat(indent_count);
    for item in items {
        match item {
            PageItem::Category(category) => {
                if let Some(url) = &category.url {
                    println!("{}Category {} -> {}", indent, category.title, url);
                } else {
                    println!("{}Category {}", indent, category.title);
                }
                dump_page(&category.contents, indent_count + 1);
            }
            PageItem::Link(link) => {
                println!("{}Link {} -> {}", indent, link.title, link.url);
            }
        }
    }
}
