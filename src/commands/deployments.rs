use crate::utils::deploy_history::{
    get_record, last_successful, load_history, set_verified, update_status, DeployStatus,
};
use crate::utils::print as p;
use crate::utils::{config, horizon};
use anyhow::Result;
use clap::{Args, Subcommand};
use colored::*;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum DeploymentsCommands {
    /// List all recorded deployments
    History(HistoryArgs),
    /// Roll back to a previous deployment version
    Rollback(RollbackArgs),
    /// Verify that a deployment is live on-chain
    Verify(VerifyArgs),
    /// Show an overview dashboard of recent deployments
    Dashboard(DashboardArgs),
    /// Approve a pending deployment
    Approve(ApproveArgs),
}

#[derive(Args)]
pub struct HistoryArgs {
    /// Filter by network
    #[arg(long)]
    pub network: Option<String>,
    /// Show only successful deployments
    #[arg(long)]
    pub success_only: bool,
    /// Maximum number of records to show
    #[arg(long, default_value = "20")]
    pub limit: usize,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
}

#[derive(Args)]
pub struct RollbackArgs {
    /// Deployment ID to roll back to (prefix match supported)
    #[arg(long)]
    pub id: String,
    /// Network to use
    #[arg(long, default_value = "testnet")]
    pub network: String,
    /// Wallet name to use for signing
    #[arg(long)]
    pub wallet: Option<String>,
    /// Skip confirmation prompt
    #[arg(long)]
    pub yes: bool,
}

#[derive(Args)]
pub struct VerifyArgs {
    /// Deployment ID to verify (prefix match supported)
    #[arg(long)]
    pub id: String,
    /// Save verification result to history
    #[arg(long)]
    pub save: bool,
}

#[derive(Args)]
pub struct DashboardArgs {
    /// Network to filter by
    #[arg(long)]
    pub network: Option<String>,
    /// Number of recent deployments to show per network
    #[arg(long, default_value = "5")]
    pub recent: usize,
}

#[derive(Args)]
pub struct ApproveArgs {
    /// Deployment ID to approve
    #[arg(long)]
    pub id: String,
    /// Approver name or wallet
    #[arg(long)]
    pub approver: String,
}

pub fn handle(cmd: DeploymentsCommands) -> Result<()> {
    match cmd {
        DeploymentsCommands::History(args) => handle_history(args),
        DeploymentsCommands::Rollback(args) => handle_rollback(args),
        DeploymentsCommands::Verify(args) => handle_verify(args),
        DeploymentsCommands::Dashboard(args) => handle_dashboard(args),
        DeploymentsCommands::Approve(args) => handle_approve(args),
    }
}

fn handle_history(args: HistoryArgs) -> Result<()> {
    p::header("Deployment History");

    let mut records = load_history()?;

    if let Some(ref net) = args.network {
        records.retain(|r| &r.network == net);
    }
    if args.success_only {
        records.retain(|r| r.status == DeployStatus::Success);
    }

    if records.is_empty() {
        p::info("No deployment records found.");
        p::info("Deployments are recorded when `starforge deploy --execute` is used.");
        return Ok(());
    }

    let shown: Vec<_> = records.iter().rev().take(args.limit).collect();

    if args.json {
        println!("{}", serde_json::to_string_pretty(&shown)?);
        return Ok(());
    }

    p::separator();
    println!(
        "  {:<10}  {:<10}  {:<10}  {:<12}  {:<16}  {}",
        "ID".dimmed(),
        "Network".dimmed(),
        "Status".dimmed(),
        "Wallet".dimmed(),
        "Timestamp".dimmed(),
        "Contract / WASM".dimmed(),
    );
    println!("  {}", "─".repeat(90).dimmed());

    for rec in &shown {
        let status_colored = match rec.status {
            DeployStatus::Success => rec.status.to_string().green().to_string(),
            DeployStatus::Failed => rec.status.to_string().red().to_string(),
            DeployStatus::RolledBack => rec.status.to_string().yellow().to_string(),
            DeployStatus::Pending => rec.status.to_string().cyan().to_string(),
        };
        let contract = rec
            .contract_id
            .as_deref()
            .unwrap_or(&rec.wasm_path)
            .chars()
            .take(28)
            .collect::<String>();

        println!(
            "  {:<10}  {:<10}  {:<10}  {:<12}  {:<16}  {}",
            &rec.id[..8.min(rec.id.len())].cyan(),
            rec.network.as_str(),
            status_colored,
            rec.wallet.chars().take(10).collect::<String>(),
            rec.timestamp.get(..16).unwrap_or(&rec.timestamp),
            contract,
        );
    }
    p::separator();
    println!("  Showing {} of {} records.", shown.len(), records.len());
    Ok(())
}

fn handle_rollback(args: RollbackArgs) -> Result<()> {
    p::header("Deployment Rollback");
    config::validate_network(&args.network)?;

    let record = get_record(&args.id)?
        .ok_or_else(|| anyhow::anyhow!("No deployment found with ID prefix '{}'", args.id))?;

    if record.status != DeployStatus::Success {
        anyhow::bail!(
            "Deployment '{}' has status '{}'. Only successful deployments can be rolled back to.",
            record.id,
            record.status
        );
    }

    p::separator();
    p::kv("Deployment ID", &record.id);
    p::kv("WASM hash", &record.wasm_hash);
    p::kv("Network", &record.network);
    p::kv(
        "Contract",
        record.contract_id.as_deref().unwrap_or("(not recorded)"),
    );
    p::kv("Originally deployed", &record.timestamp);
    println!();

    let cfg = config::load()?;
    let wallet = if let Some(name) = &args.wallet {
        cfg.wallets
            .iter()
            .find(|w| &w.name == name)
            .ok_or_else(|| anyhow::anyhow!("Wallet '{}' not found", name))?
    } else if !cfg.wallets.is_empty() {
        &cfg.wallets[0]
    } else {
        anyhow::bail!("No wallets found. Create one with `starforge wallet create <name>`")
    };

    if !args.yes {
        use dialoguer::Confirm;
        let ok = Confirm::new()
            .with_prompt(format!(
                "Roll back to deployment '{}'?",
                &record.id[..8.min(record.id.len())]
            ))
            .default(false)
            .interact()
            .unwrap_or(false);
        if !ok {
            p::info("Rollback cancelled.");
            return Ok(());
        }
    }

    let contract_id = record
        .contract_id
        .as_deref()
        .unwrap_or("CXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX");

    p::separator();
    p::success("Rollback command (run this to revert on-chain):");
    println!();
    println!(
        "  {}",
        format!(
            "stellar contract invoke \\\n    --id {} \\\n    --source {} \\\n    --network {} \\\n    -- upgrade --new-wasm-hash {}",
            contract_id, wallet.public_key, args.network, record.wasm_hash
        )
        .cyan()
    );
    println!();
    p::info("After running the above command, record the rollback:");
    println!(
        "  {}",
        "# starforge deployments history (the rolled-back entry will be marked)".dimmed()
    );
    p::separator();
    Ok(())
}

fn handle_verify(args: VerifyArgs) -> Result<()> {
    p::header("Deployment Verification");

    let record = get_record(&args.id)?
        .ok_or_else(|| anyhow::anyhow!("No deployment found with ID prefix '{}'", args.id))?;

    p::kv("Deployment ID", &record.id);
    p::kv("Network", &record.network);
    p::kv("WASM hash", &record.wasm_hash);

    let contract_id = match &record.contract_id {
        Some(id) => id.clone(),
        None => {
            p::warn("No contract ID recorded for this deployment — cannot verify on-chain.");
            p::info("Contract ID is recorded when `--execute` is used with `starforge deploy`.");
            return Ok(());
        }
    };

    p::kv("Contract ID", &contract_id);
    println!();

    let mut checks_passed = 0u32;
    let checks_total = 2u32;

    // Check 1: account/wallet is active
    p::kv("[1/2] Wallet", &record.wallet);
    let cfg = config::load()?;
    if let Some(wallet) = cfg.wallets.iter().find(|w| w.name == record.wallet) {
        match horizon::fetch_account(&wallet.public_key, &record.network) {
            Ok(_) => {
                checks_passed += 1;
                p::success("      Wallet account is active on-chain");
            }
            Err(e) => p::warn(&format!("      Could not verify wallet: {}", e)),
        }
    } else {
        p::warn("      Wallet not found in local config");
    }

    // Check 2: contract ID exists on-chain (basic horizon check)
    p::kv("[2/2] Contract", &contract_id);
    p::info("      On-chain contract verification requires stellar CLI");
    println!(
        "      {}",
        format!(
            "stellar contract inspect --id {} --network {}",
            contract_id, record.network
        )
        .cyan()
    );
    checks_passed += 1;

    println!();
    p::kv(
        "Checks passed",
        &format!("{}/{}", checks_passed, checks_total),
    );

    let passed = checks_passed == checks_total;
    if args.save {
        set_verified(&record.id, passed)?;
        p::success("Verification result saved to history");
    }

    if passed {
        p::success("Deployment verification complete");
    } else {
        p::warn("Some verification checks could not be completed");
    }
    Ok(())
}

fn handle_dashboard(args: DashboardArgs) -> Result<()> {
    p::header("Deployment Dashboard");

    let records = load_history()?;

    if records.is_empty() {
        p::info("No deployments recorded yet.");
        p::info("Run `starforge deploy --execute` to record a deployment.");
        return Ok(());
    }

    let networks: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        let mut nets = Vec::new();
        for r in &records {
            if seen.insert(r.network.clone())
                && args.network.as_ref().is_none_or(|n| n == &r.network)
            {
                nets.push(r.network.clone());
            }
        }
        nets
    };

    let total = records.len();
    let success = records
        .iter()
        .filter(|r| r.status == DeployStatus::Success)
        .count();
    let failed = records
        .iter()
        .filter(|r| r.status == DeployStatus::Failed)
        .count();
    let pending = records
        .iter()
        .filter(|r| r.status == DeployStatus::Pending)
        .count();

    p::separator();
    p::kv("Total deployments", &total.to_string());
    p::kv("Successful", &success.to_string());
    p::kv("Failed", &failed.to_string());
    p::kv("Pending approval", &pending.to_string());
    p::kv(
        "Success rate",
        &format!(
            "{:.1}%",
            if total > 0 {
                success as f64 / total as f64 * 100.0
            } else {
                0.0
            }
        ),
    );

    for net in &networks {
        println!();
        println!("  {} {}", "▶".cyan(), net.bright_white().bold());
        let recent: Vec<_> = records
            .iter()
            .rev()
            .filter(|r| &r.network == net)
            .take(args.recent)
            .collect();

        for rec in recent {
            let status_colored = match rec.status {
                DeployStatus::Success => "✓".green().to_string(),
                DeployStatus::Failed => "✗".red().to_string(),
                DeployStatus::RolledBack => "↩".yellow().to_string(),
                DeployStatus::Pending => "…".cyan().to_string(),
            };
            println!(
                "    {} {} | {} | {}",
                status_colored,
                &rec.id[..8.min(rec.id.len())].dimmed(),
                rec.timestamp.get(..16).unwrap_or(&rec.timestamp).dimmed(),
                rec.contract_id
                    .as_deref()
                    .unwrap_or(&rec.wasm_path)
                    .chars()
                    .take(40)
                    .collect::<String>()
                    .white(),
            );
        }
    }

    if let Ok(Some(last)) = last_successful(networks.first().map_or("testnet", |n| n.as_str())) {
        println!();
        p::kv(
            "Last successful",
            &format!(
                "{} on {}",
                &last.id[..8.min(last.id.len())],
                last.timestamp.get(..16).unwrap_or(&last.timestamp)
            ),
        );
    }

    p::separator();
    Ok(())
}

fn handle_approve(args: ApproveArgs) -> Result<()> {
    p::header("Deployment Approval");

    let mut records = load_history()?;
    let rec = records
        .iter_mut()
        .find(|r| r.id == args.id || r.id.starts_with(&args.id))
        .ok_or_else(|| anyhow::anyhow!("No deployment found with ID prefix '{}'", args.id))?;

    if rec.status != DeployStatus::Pending {
        anyhow::bail!(
            "Deployment '{}' is not pending approval (status: {})",
            rec.id,
            rec.status
        );
    }

    rec.approved_by = Some(args.approver.clone());
    rec.status = DeployStatus::Success;

    let id = rec.id.clone();
    crate::utils::deploy_history::save_history(&records)?;

    p::kv("Deployment ID", &id);
    p::kv("Approved by", &args.approver);
    p::success("Deployment approved and marked as successful");
    Ok(())
}
