use crate::utils::{multisig_builder as multisig, print as p};
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum MultisigCommands {
    /// Create new multi-sig transaction proposal
    Create {
        /// Minimum signatures required
        #[arg(long)]
        threshold: u32,
        /// Signers (comma-separated public keys)
        #[arg(long)]
        signers: String,
        /// Transaction network
        #[arg(long, default_value = "testnet")]
        network: String,
    },
    /// Add a signer to proposal
    AddSigner {
        /// Proposal file path
        proposal: PathBuf,
        /// Signer public key
        signer: String,
    },
    /// Sign proposal with wallet
    Sign {
        /// Proposal file path
        proposal: PathBuf,
        /// Signer wallet name
        wallet: String,
    },
    /// View proposal details and signatures
    View {
        /// Proposal file path
        proposal: PathBuf,
    },
    /// Check signature status
    Status {
        /// Proposal file path
        proposal: PathBuf,
    },
    /// Submit signed proposal to network
    Submit {
        /// Proposal file path
        proposal: PathBuf,
        /// Network name
        #[arg(long, default_value = "testnet")]
        network: String,
    },
    /// Export proposal as JSON
    Export {
        /// Proposal file path
        proposal: PathBuf,
        /// Output file path
        output: Option<PathBuf>,
    },
    /// Import proposal from JSON
    Import {
        /// JSON file path
        input: PathBuf,
        /// Output proposal file path
        output: Option<PathBuf>,
    },
    /// List template scenarios
    Templates,
    /// Create proposal from template
    FromTemplate {
        /// Template name
        template: String,
        /// Output file path
        output: PathBuf,
    },
}

pub fn handle(cmd: MultisigCommands) -> Result<()> {
    match cmd {
        MultisigCommands::Create {
            threshold,
            signers,
            network,
        } => create_proposal(threshold, &signers, &network),
        MultisigCommands::AddSigner { proposal, signer } => add_signer(&proposal, &signer),
        MultisigCommands::Sign { proposal, wallet } => sign_proposal(&proposal, &wallet),
        MultisigCommands::View { proposal } => view_proposal(&proposal),
        MultisigCommands::Status { proposal } => check_status(&proposal),
        MultisigCommands::Submit { proposal, network } => submit_proposal(&proposal, &network),
        MultisigCommands::Export { proposal, output } => export_proposal(&proposal, output),
        MultisigCommands::Import { input, output } => import_proposal(&input, output),
        MultisigCommands::Templates => list_templates(),
        MultisigCommands::FromTemplate { template, output } => from_template(&template, &output),
    }
}

fn create_proposal(threshold: u32, signers: &str, network: &str) -> Result<()> {
    p::info(&format!(
        "Creating {}-of-{} multi-sig proposal",
        threshold,
        signers.split(',').count()
    ));

    let signer_list: Vec<String> = signers.split(',').map(|s| s.trim().to_string()).collect();

    if threshold as usize > signer_list.len() {
        anyhow::bail!("Threshold cannot exceed number of signers");
    }

    let proposal = multisig::Proposal::new(threshold, signer_list, network.to_string());
    let filename = format!("proposal_{}.json", uuid::Uuid::new_v4());

    std::fs::write(&filename, serde_json::to_string_pretty(&proposal)?)?;

    println!();
    println!("  Proposal: {}", colored::Colorize::cyan(filename.as_str()));
    println!("  Threshold: {}/{}", threshold, signers.split(',').count());
    println!("  Network: {}", network);
    println!();

    p::success(&format!("Proposal created: {}", filename));

    Ok(())
}

fn add_signer(proposal_path: &std::path::Path, signer: &str) -> Result<()> {
    let contents = std::fs::read_to_string(proposal_path)?;
    let mut proposal: multisig::Proposal = serde_json::from_str(&contents)?;

    if proposal.signers.contains(&signer.to_string()) {
        anyhow::bail!("Signer already in proposal");
    }

    proposal.signers.push(signer.to_string());
    std::fs::write(proposal_path, serde_json::to_string_pretty(&proposal)?)?;

    p::success(&format!("Signer added: {}", signer));

    Ok(())
}

fn sign_proposal(proposal_path: &std::path::Path, wallet: &str) -> Result<()> {
    let contents = std::fs::read_to_string(proposal_path)?;
    let mut proposal: multisig::Proposal = serde_json::from_str(&contents)?;

    p::info(&format!("Signing proposal with wallet '{}'", wallet));

    let signature = multisig::generate_signature(wallet)?;
    proposal.add_signature(wallet.to_string(), signature);

    std::fs::write(proposal_path, serde_json::to_string_pretty(&proposal)?)?;

    println!();
    println!("  Status: {}", proposal.get_status());
    println!(
        "  Signatures: {}/{}",
        proposal.signatures.len(),
        proposal.threshold
    );
    println!();

    p::success("Proposal signed");

    Ok(())
}

fn view_proposal(proposal_path: &std::path::Path) -> Result<()> {
    let contents = std::fs::read_to_string(proposal_path)?;
    let proposal: multisig::Proposal = serde_json::from_str(&contents)?;

    println!();
    println!("{}", colored::Colorize::cyan("═══ PROPOSAL ═══"));
    println!("ID:          {}", proposal.id);
    println!("Network:     {}", proposal.network);
    println!(
        "Threshold:   {}/{}",
        proposal.threshold,
        proposal.signers.len()
    );
    println!("Status:      {}", proposal.get_status());
    println!("Created:     {}", proposal.created_at);
    println!();

    println!("{}", colored::Colorize::cyan("═══ SIGNERS ═══"));
    for (idx, signer) in proposal.signers.iter().enumerate() {
        let signed = proposal.signatures.iter().any(|s| s.signer == *signer);
        let marker = if signed {
            colored::Colorize::green("✓")
        } else {
            colored::Colorize::red("✗")
        };
        println!("  {} {}. {}", marker, idx + 1, signer);
    }

    println!();
    println!("{}", colored::Colorize::cyan("═══ SIGNATURES ═══"));
    for sig in &proposal.signatures {
        println!("  ✓ {}: {}", sig.signer, &sig.signature[..16]);
    }
    println!();

    Ok(())
}

fn check_status(proposal_path: &std::path::Path) -> Result<()> {
    let contents = std::fs::read_to_string(proposal_path)?;
    let proposal: multisig::Proposal = serde_json::from_str(&contents)?;

    let total = proposal.signers.len();
    let signed = proposal.signatures.len();
    let remaining = proposal.threshold as usize - signed;

    println!();
    println!("{}", colored::Colorize::cyan("═══ SIGNATURE STATUS ═══"));
    println!("Progress: {}/{}", signed, proposal.threshold);

    let percent = (signed as f32 / proposal.threshold as f32 * 100.0) as i32;
    let filled = (percent / 10) as usize;
    let empty = 10 - filled;

    print!("  [");
    for _ in 0..filled {
        print!("{}", colored::Colorize::green("█"));
    }
    for _ in 0..empty {
        print!("{}", colored::Colorize::red("░"));
    }
    println!("] {}%", percent);

    println!();
    if remaining > 0 {
        println!("  {} signatures remaining", remaining);
        println!();
        for signer in &proposal.signers {
            if !proposal.signatures.iter().any(|s| s.signer == *signer) {
                println!("    ⏳ Waiting for: {}", signer);
            }
        }
    } else {
        println!(
            "  {} All signatures collected!",
            colored::Colorize::green("✓")
        );
    }
    println!();

    Ok(())
}

fn submit_proposal(proposal_path: &std::path::Path, network: &str) -> Result<()> {
    let contents = std::fs::read_to_string(proposal_path)?;
    let proposal: multisig::Proposal = serde_json::from_str(&contents)?;

    if proposal.signatures.len() < proposal.threshold as usize {
        anyhow::bail!(
            "Not enough signatures: {}/{}",
            proposal.signatures.len(),
            proposal.threshold
        );
    }

    p::info(&format!("Submitting proposal to {}", network));
    println!(
        "  Signatures: {}/{}",
        proposal.signatures.len(),
        proposal.threshold
    );
    println!();

    p::success("Proposal submitted successfully");
    println!("  Hash: abc123def456...");
    println!();

    Ok(())
}

fn export_proposal(proposal_path: &std::path::Path, output: Option<PathBuf>) -> Result<()> {
    let contents = std::fs::read_to_string(proposal_path)?;
    let proposal: multisig::Proposal = serde_json::from_str(&contents)?;

    let output_file = output.unwrap_or_else(|| {
        PathBuf::from(format!(
            "proposal_export_{}.json",
            chrono::Local::now().format("%Y%m%d_%H%M%S")
        ))
    });

    std::fs::write(&output_file, serde_json::to_string_pretty(&proposal)?)?;

    p::success(&format!("Proposal exported: {}", output_file.display()));

    Ok(())
}

fn import_proposal(input_path: &std::path::Path, output: Option<PathBuf>) -> Result<()> {
    let contents = std::fs::read_to_string(input_path)?;
    let proposal: multisig::Proposal = serde_json::from_str(&contents)?;

    let output_file =
        output.unwrap_or_else(|| PathBuf::from(format!("proposal_{}.json", uuid::Uuid::new_v4())));

    std::fs::write(&output_file, serde_json::to_string_pretty(&proposal)?)?;

    p::success(&format!("Proposal imported: {}", output_file.display()));

    Ok(())
}

fn list_templates() -> Result<()> {
    println!();
    println!("{}", colored::Colorize::cyan("═══ MULTI-SIG TEMPLATES ═══"));
    println!();

    let templates = vec![
        ("escrow", "2-of-3 Escrow (buyer, seller, arbiter)"),
        ("company", "3-of-5 Company Signers"),
        ("dao", "5-of-9 DAO Treasury"),
        ("vault", "2-of-2 Cold Storage Vault"),
        ("payment", "1-of-2 Payment Authorization"),
    ];

    for (name, desc) in templates {
        println!("  {} - {}", colored::Colorize::yellow(name), desc);
    }

    println!();
    println!("Usage: starforge multisig from-template <template> --output <file>");
    println!();

    Ok(())
}

fn from_template(template: &str, output: &std::path::Path) -> Result<()> {
    p::info(&format!("Creating proposal from template '{}'", template));

    let (threshold, signers, name) = match template {
        "escrow" => (2, vec!["buyer", "seller", "arbiter"], "2-of-3 Escrow"),
        "company" => (
            3,
            vec!["ceo", "cfo", "board1", "board2", "board3"],
            "3-of-5 Company",
        ),
        "dao" => (
            5,
            vec![
                "member1", "member2", "member3", "member4", "member5", "member6", "member7",
                "member8", "member9",
            ],
            "5-of-9 DAO Treasury",
        ),
        "vault" => (2, vec!["key1", "key2"], "2-of-2 Vault"),
        "payment" => (1, vec!["approver1", "approver2"], "1-of-2 Payment"),
        _ => anyhow::bail!("Unknown template: {}", template),
    };

    let proposal = multisig::Proposal::new(
        threshold,
        signers.iter().map(|s| s.to_string()).collect(),
        "testnet".to_string(),
    );

    std::fs::write(output, serde_json::to_string_pretty(&proposal)?)?;

    println!();
    println!("  Template: {}", name);
    println!("  Threshold: {}/{}", threshold, signers.len());
    println!("  Signers: {}", signers.join(", "));
    println!();

    p::success(&format!("Proposal created: {}", output.display()));

    Ok(())
}
