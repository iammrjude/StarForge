use crate::utils::{docs, print as p};
use anyhow::Result;
use clap::Subcommand;
use colored::Colorize;

#[derive(Subcommand)]
pub enum DocsCommands {
    /// Generate documentation for a contract
    Generate {
        /// Contract ID
        contract: String,
        /// Contract name
        #[arg(long)]
        name: String,
        /// Contract description
        #[arg(long)]
        description: String,
        /// Network
        #[arg(long, default_value = "testnet")]
        network: String,
        /// Documentation version
        #[arg(long, default_value = "1.0.0")]
        version: String,
    },
    /// Show documentation for a contract
    Show {
        /// Contract ID
        contract: String,
        /// Specific version to show (latest if omitted)
        #[arg(long)]
        version: Option<String>,
    },
    /// List all documented contracts
    List,
    /// Search across all documentation
    Search {
        /// Search query
        query: String,
    },
    /// Show documentation versions for a contract
    Versions {
        /// Contract ID
        contract: String,
    },
    /// Render documentation as Markdown
    Export {
        /// Contract ID
        contract: String,
        /// Version to export (latest if omitted)
        #[arg(long)]
        version: Option<String>,
    },
}

pub fn handle(cmd: DocsCommands) -> Result<()> {
    match cmd {
        DocsCommands::Generate {
            contract,
            name,
            description,
            network,
            version,
        } => generate(contract, name, description, network, version),
        DocsCommands::Show { contract, version } => show(contract, version),
        DocsCommands::List => list(),
        DocsCommands::Search { query } => search(query),
        DocsCommands::Versions { contract } => versions(contract),
        DocsCommands::Export { contract, version } => export(contract, version),
    }
}

fn generate(
    contract: String,
    name: String,
    description: String,
    network: String,
    version: String,
) -> Result<()> {
    p::header("Documentation Portal — Generate");

    p::step(1, 3, "Generating documentation structure...");
    let functions = vec![
        docs::FunctionDoc {
            name: "initialize".to_string(),
            description: "Initialize the contract with admin address".to_string(),
            parameters: vec![docs::ParamDoc {
                name: "admin".to_string(),
                ty: "Address".to_string(),
                description: "The admin address".to_string(),
                required: true,
            }],
            returns: Some("bool".to_string()),
            examples: vec!["contract.initialize(&admin)".to_string()],
        },
        docs::FunctionDoc {
            name: "transfer".to_string(),
            description: "Transfer tokens between accounts".to_string(),
            parameters: vec![
                docs::ParamDoc {
                    name: "from".to_string(),
                    ty: "Address".to_string(),
                    description: "Source address".to_string(),
                    required: true,
                },
                docs::ParamDoc {
                    name: "to".to_string(),
                    ty: "Address".to_string(),
                    description: "Destination address".to_string(),
                    required: true,
                },
                docs::ParamDoc {
                    name: "amount".to_string(),
                    ty: "i128".to_string(),
                    description: "Amount to transfer".to_string(),
                    required: true,
                },
            ],
            returns: Some("bool".to_string()),
            examples: vec!["contract.transfer(&from, &to, 1000)".to_string()],
        },
    ];

    let events = vec![docs::EventDoc {
        name: "Transfer".to_string(),
        description: "Emitted on token transfer".to_string(),
        topics: vec![
            docs::TopicDoc {
                name: "from".to_string(),
                ty: "Address".to_string(),
                description: "Source address".to_string(),
            },
            docs::TopicDoc {
                name: "to".to_string(),
                ty: "Address".to_string(),
                description: "Destination address".to_string(),
            },
        ],
    }];

    let storage = vec![
        docs::StorageDoc {
            key: "admin".to_string(),
            ty: "Address".to_string(),
            description: "Contract administrator address".to_string(),
        },
        docs::StorageDoc {
            key: "balances".to_string(),
            ty: "Map<Address, i128>".to_string(),
            description: "Token balances for all accounts".to_string(),
        },
    ];

    let sections = vec![
        docs::DocSection {
            title: "Overview".to_string(),
            content: format!(
                "{} is a Soroban smart contract deployed on {}. {}",
                name, network, description
            ),
            order: 0,
        },
        docs::DocSection {
            title: "Getting Started".to_string(),
            content: format!(
                "To interact with {}, deploy it to {} and call its functions via the Soroban RPC.",
                name, network
            ),
            order: 1,
        },
        docs::DocSection {
            title: "Security".to_string(),
            content: "This contract uses address-based authorization. All state-changing operations require the caller to be the authorized address.".to_string(),
            order: 2,
        },
    ];

    p::step(2, 3, "Writing documentation files...");
    let entry = docs::generate_documentation(
        &contract,
        &name,
        &description,
        &network,
        &version,
        functions,
        events,
        storage,
        sections,
    )?;

    p::step(3, 3, "Updating documentation index...");
    println!();
    p::success(&format!("Documentation generated for '{}'", name));
    p::kv("Contract", &entry.contract_id);
    p::kv("Version", &entry.version);
    p::kv("Network", &entry.network);
    p::kv("Generated", &entry.generated_at[..10]);
    p::info("Use `starforge docs show` to view the documentation.");
    Ok(())
}

fn show(contract: String, version: Option<String>) -> Result<()> {
    p::header("Documentation Portal — View");

    let entry = docs::get_documentation(&contract, version.as_deref())?;

    p::separator();
    p::kv_accent("Contract", &entry.name);
    p::kv("ID", &entry.contract_id);
    p::kv("Version", &entry.version);
    p::kv("Network", &entry.network);
    p::kv("Generated", &entry.generated_at[..10]);
    p::separator();

    println!();
    for section in &entry.sections {
        println!("  {} {}", "##".dimmed(), section.title.bright_white());
        println!("  {}", section.content.dimmed());
        println!();
    }

    if !entry.api.functions.is_empty() {
        p::info("API Reference — Functions");
        for func in &entry.api.functions {
            println!("  {} `{}`", "→".cyan(), func.name.bright_white());
            println!("    {}", func.description);
            if !func.parameters.is_empty() {
                for param in &func.parameters {
                    let req = if param.required {
                        "required"
                    } else {
                        "optional"
                    };
                    println!(
                        "    • {} ({}): {} [{}]",
                        param.name, param.ty, param.description, req
                    );
                }
            }
            if let Some(ref returns) = func.returns {
                println!("    Returns: {}", returns);
            }
            println!();
        }
    }

    if !entry.api.events.is_empty() {
        p::info("API Reference — Events");
        for event in &entry.api.events {
            println!("  {} `{}`", "→".cyan(), event.name.bright_white());
            println!("    {}", event.description);
            for topic in &event.topics {
                println!("    • {} ({}): {}", topic.name, topic.ty, topic.description);
            }
            println!();
        }
    }

    if !entry.api.storage.is_empty() {
        p::info("Storage Layout");
        for storage in &entry.api.storage {
            println!(
                "  • {} ({}): {}",
                storage.key, storage.ty, storage.description
            );
        }
    }

    println!();
    p::separator();
    Ok(())
}

fn list() -> Result<()> {
    p::header("Documentation Portal — Index");

    let index = docs::list_documentation()?;

    if index.contracts.is_empty() {
        p::info("No documentation generated yet. Use `starforge docs generate` first.");
        return Ok(());
    }

    for contract in &index.contracts {
        println!(
            "  {} {} ({} versions)",
            "→".cyan(),
            contract.name.bright_white(),
            contract.versions.len()
        );
        p::kv("Contract ID", &contract.contract_id);
        if let Some(latest) = contract.versions.first() {
            p::kv("Latest", &latest.version);
        }
        println!();
    }

    p::kv("Total", &index.contracts.len().to_string());
    Ok(())
}

fn search(query: String) -> Result<()> {
    p::header(&format!("Documentation Search: '{}'", query));

    let results = docs::search_documentation(&query)?;

    if results.is_empty() {
        p::info("No documentation matched your search query.");
        return Ok(());
    }

    p::kv("Matches", &results.len().to_string());
    println!();

    for result in &results {
        println!(
            "  {} {} (score: {})",
            "→".cyan(),
            result.name.bright_white(),
            result.score
        );
        p::kv("Contract", &result.contract_id);
        p::kv("Version", &result.version);
        if !result.matched_sections.is_empty() {
            p::kv("Matched", &result.matched_sections.join(", "));
        }
        println!();
    }

    Ok(())
}

fn versions(contract: String) -> Result<()> {
    p::header("Documentation Portal — Versions");
    p::kv("Contract", &contract);

    let versions = docs::list_versions(&contract)?;

    if versions.is_empty() {
        p::info("No documentation versions found for this contract.");
        return Ok(());
    }

    println!();
    for version in &versions {
        println!("  {} v{}", "→".cyan(), version.bright_white());
    }

    println!();
    p::kv("Versions", &versions.len().to_string());
    Ok(())
}

fn export(contract: String, version: Option<String>) -> Result<()> {
    p::header("Documentation Portal — Export Markdown");

    let md = docs::render_markdown(&contract, version.as_deref())?;
    println!("{}", md);

    Ok(())
}
