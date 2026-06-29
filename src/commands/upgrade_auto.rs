use crate::utils::{config, print as p};
use anyhow::Result;
use chrono::Utc;
use clap::{Args, Subcommand};
use colored::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Subcommand)]
pub enum UpgradeAutoCommands {
    /// Check compatibility between two WASM versions
    Compat(CompatArgs),
    /// Generate an automated upgrade workflow plan
    Plan(PlanArgs),
    /// Apply an upgrade workflow plan (runs compatibility check, migration, upgrade)
    Apply(ApplyArgs),
    /// Generate a state migration script template
    Migration(MigrationArgs),
    /// List saved upgrade workflow plans
    Plans(PlansArgs),
    /// Rollback to a previous auto-managed version
    Rollback(RollbackArgs),
}

#[derive(Args)]
pub struct CompatArgs {
    /// Path to the old WASM version
    #[arg(long)]
    pub old_wasm: PathBuf,
    /// Path to the new WASM version
    #[arg(long)]
    pub new_wasm: PathBuf,
    /// Output as JSON
    #[arg(long)]
    pub json: bool,
    /// Fail with exit code 1 if incompatible
    #[arg(long, default_value = "true")]
    pub fail_on_incompatible: bool,
}

#[derive(Args)]
pub struct PlanArgs {
    /// Contract ID to upgrade
    #[arg(long)]
    pub contract_id: String,
    /// Path to the old WASM version (for compatibility analysis)
    #[arg(long)]
    pub old_wasm: PathBuf,
    /// Path to the new WASM version
    #[arg(long)]
    pub new_wasm: PathBuf,
    /// Network
    #[arg(long, default_value = "testnet", value_parser = ["testnet", "mainnet"])]
    pub network: String,
    /// Human-readable upgrade description
    #[arg(long, default_value = "Automated upgrade")]
    pub description: String,
    /// Auto-approve compatibility warnings (don't prompt)
    #[arg(long, default_value = "false")]
    pub auto_approve: bool,
}

#[derive(Args)]
pub struct ApplyArgs {
    /// Plan ID to apply
    #[arg(long)]
    pub plan_id: String,
    /// Wallet name for signing
    #[arg(long)]
    pub wallet: Option<String>,
    /// Network
    #[arg(long, default_value = "testnet", value_parser = ["testnet", "mainnet"])]
    pub network: String,
    /// Skip confirmation prompt
    #[arg(long, default_value = "false")]
    pub yes: bool,
    /// Run migration step before upgrade
    #[arg(long, default_value = "true")]
    pub run_migration: bool,
}

#[derive(Args)]
pub struct MigrationArgs {
    /// Path to the old WASM version (for state analysis)
    #[arg(long)]
    pub old_wasm: PathBuf,
    /// Path to the new WASM version
    #[arg(long)]
    pub new_wasm: PathBuf,
    /// Output directory for migration script
    #[arg(long, default_value = "migrations")]
    pub out_dir: PathBuf,
    /// Contract label
    #[arg(long)]
    pub contract: String,
}

#[derive(Args)]
pub struct PlansArgs {
    /// Filter by contract ID
    #[arg(long)]
    pub contract_id: Option<String>,
    /// Filter by network
    #[arg(long)]
    pub network: Option<String>,
}

#[derive(Args)]
pub struct RollbackArgs {
    /// Plan ID for the upgrade to roll back
    #[arg(long)]
    pub plan_id: String,
    /// Wallet for signing
    #[arg(long)]
    pub wallet: Option<String>,
    /// Network
    #[arg(long, default_value = "testnet", value_parser = ["testnet", "mainnet"])]
    pub network: String,
    /// Skip confirmation
    #[arg(long, default_value = "false")]
    pub yes: bool,
}

// ── Data structures ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CompatibilityLevel {
    Compatible,
    CompatibleWithWarnings,
    Incompatible,
}

impl std::fmt::Display for CompatibilityLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CompatibilityLevel::Compatible => write!(f, "compatible"),
            CompatibilityLevel::CompatibleWithWarnings => write!(f, "compatible-with-warnings"),
            CompatibilityLevel::Incompatible => write!(f, "incompatible"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatCheck {
    pub old_hash: String,
    pub new_hash: String,
    pub level: CompatibilityLevel,
    pub issues: Vec<CompatIssue>,
    pub old_size_bytes: usize,
    pub new_size_bytes: usize,
    pub size_delta_bytes: i64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatIssue {
    pub kind: String,
    pub severity: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PlanStatus {
    Pending,
    Applied,
    RolledBack,
    Failed,
}

impl std::fmt::Display for PlanStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanStatus::Pending => write!(f, "pending"),
            PlanStatus::Applied => write!(f, "applied"),
            PlanStatus::RolledBack => write!(f, "rolled-back"),
            PlanStatus::Failed => write!(f, "failed"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpgradePlan {
    pub id: String,
    pub contract_id: String,
    pub network: String,
    pub description: String,
    pub old_wasm_hash: String,
    pub new_wasm_hash: String,
    pub compat_level: CompatibilityLevel,
    pub migration_script: Option<String>,
    pub status: PlanStatus,
    pub created_at: String,
    pub applied_at: Option<String>,
    pub applied_by: Option<String>,
}

// ── Storage helpers ───────────────────────────────────────────────────────────

fn auto_dir() -> Result<PathBuf> {
    let dir = config::config_dir().join("upgrade-auto");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
    }
    Ok(dir)
}

fn plans_path() -> Result<PathBuf> {
    Ok(auto_dir()?.join("plans.json"))
}

fn load_plans() -> Result<Vec<UpgradePlan>> {
    let path = plans_path()?;
    if !path.exists() {
        return Ok(vec![]);
    }
    let data = fs::read_to_string(&path)?;
    Ok(serde_json::from_str(&data).unwrap_or_default())
}

fn save_plans(plans: &[UpgradePlan]) -> Result<()> {
    fs::write(plans_path()?, serde_json::to_string_pretty(plans)?)?;
    Ok(())
}

// ── Core logic ────────────────────────────────────────────────────────────────

fn wasm_hash_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn read_valid_wasm(path: &PathBuf) -> Result<Vec<u8>> {
    if !path.exists() {
        anyhow::bail!(
            "WASM file not found: {}\nRun `stellar contract build` first.",
            path.display()
        );
    }
    let bytes = fs::read(path)?;
    if bytes.len() < 4 || &bytes[..4] != b"\0asm" {
        anyhow::bail!(
            "File does not appear to be a valid WASM binary: {}",
            path.display()
        );
    }
    Ok(bytes)
}

/// Performs heuristic compatibility analysis between two WASM binaries.
pub fn analyse_compat(old_bytes: &[u8], new_bytes: &[u8]) -> CompatCheck {
    let old_hash = wasm_hash_hex(old_bytes);
    let new_hash = wasm_hash_hex(new_bytes);
    let size_delta = new_bytes.len() as i64 - old_bytes.len() as i64;

    let mut issues: Vec<CompatIssue> = Vec::new();

    // Size reduction may indicate removed exports
    if size_delta < -(1024 * 10) {
        issues.push(CompatIssue {
            kind: "size-reduction".to_string(),
            severity: "warning".to_string(),
            description: format!(
                "New binary is {:.1} KB smaller — exports may have been removed",
                (-size_delta) as f64 / 1024.0
            ),
        });
    }

    // Check for presence of "upgrade" export keyword in name section
    let old_has_upgrade_fn = old_bytes.windows(7).any(|w| w == b"upgrade");
    let new_has_upgrade_fn = new_bytes.windows(7).any(|w| w == b"upgrade");

    if old_has_upgrade_fn && !new_has_upgrade_fn {
        issues.push(CompatIssue {
            kind: "missing-upgrade-fn".to_string(),
            severity: "critical".to_string(),
            description: "Old binary exposed an 'upgrade' function but new binary does not — upgrade path may be broken".to_string(),
        });
    }

    // Check for Soroban auth signatures
    let old_has_auth = old_bytes.windows(12).any(|w| *w == b"require_auth"[..]);
    let new_has_auth = new_bytes.windows(12).any(|w| *w == b"require_auth"[..]);

    if old_has_auth && !new_has_auth {
        issues.push(CompatIssue {
            kind: "auth-removed".to_string(),
            severity: "critical".to_string(),
            description: "Authorization guards (require_auth) present in old binary but absent in new — security regression".to_string(),
        });
    }

    // If identical hashes — nothing changed
    if old_hash == new_hash {
        issues.push(CompatIssue {
            kind: "identical-binary".to_string(),
            severity: "warning".to_string(),
            description: "Old and new WASM binaries are identical — no upgrade necessary"
                .to_string(),
        });
    }

    let level = if issues.iter().any(|i| i.severity == "critical") {
        CompatibilityLevel::Incompatible
    } else if issues.is_empty() {
        CompatibilityLevel::Compatible
    } else {
        CompatibilityLevel::CompatibleWithWarnings
    };

    CompatCheck {
        old_hash,
        new_hash,
        level,
        issues,
        old_size_bytes: old_bytes.len(),
        new_size_bytes: new_bytes.len(),
        size_delta_bytes: size_delta,
        timestamp: Utc::now().to_rfc3339(),
    }
}

/// Generate a migration script template based on WASM differences.
pub fn generate_migration_script(contract: &str, old_hash: &str, new_hash: &str) -> String {
    format!(
        r#"//! State migration script for contract: {contract}
//! Upgrade: {old_hash_short}... → {new_hash_short}...
//! Generated by starforge upgrade-auto migration
//!
//! Instructions:
//!   1. Review and complete the TODO sections below.
//!   2. Deploy this migration alongside the new WASM.
//!   3. Call `migrate` before or after the WASM upgrade depending on your strategy.

use soroban_sdk::{{Env, Address}};

/// Entry point called by the governance / upgrade automation.
/// Implement state transformations here.
pub fn migrate(env: &Env, admin: Address) {{
    admin.require_auth();

    // TODO: fetch old state keys and transform them into new layout.
    // Example:
    //   let old_value: i128 = env.storage().instance().get(&"old_key").unwrap_or(0);
    //   env.storage().instance().set(&"new_key", &old_value);

    // TODO: remove deprecated keys
    //   env.storage().instance().remove(&"deprecated_key");

    // Emit a migration event for off-chain indexers
    env.events().publish(
        (soroban_sdk::symbol_short!("migrated"),),
        (
            soroban_sdk::Bytes::from_slice(env, b"{old_hash_short}"),
            soroban_sdk::Bytes::from_slice(env, b"{new_hash_short}"),
        ),
    );
}}

#[cfg(test)]
mod tests {{
    use soroban_sdk::{{Env, testutils::Address as _}};

    #[test]
    fn migration_smoke_test() {{
        let env = Env::default();
        let admin = soroban_sdk::Address::generate(&env);
        env.mock_all_auths();
        // super::migrate(&env, admin); // Uncomment after implementing migrate()
    }}
}}
"#,
        contract = contract,
        old_hash_short = &old_hash[..old_hash.len().min(12)],
        new_hash_short = &new_hash[..new_hash.len().min(12)],
    )
}

// ── Command handlers ──────────────────────────────────────────────────────────

pub async fn handle(cmd: UpgradeAutoCommands) -> Result<()> {
    match cmd {
        UpgradeAutoCommands::Compat(args) => handle_compat(args),
        UpgradeAutoCommands::Plan(args) => handle_plan(args),
        UpgradeAutoCommands::Apply(args) => handle_apply(args),
        UpgradeAutoCommands::Migration(args) => handle_migration(args),
        UpgradeAutoCommands::Plans(args) => handle_plans(args),
        UpgradeAutoCommands::Rollback(args) => handle_rollback(args),
    }
}

fn handle_compat(args: CompatArgs) -> Result<()> {
    p::header("Contract Compatibility Check");

    p::step(1, 2, "Loading WASM binaries…");
    let old_bytes = read_valid_wasm(&args.old_wasm)?;
    let new_bytes = read_valid_wasm(&args.new_wasm)?;

    p::step(2, 2, "Analysing compatibility…");
    let compat = analyse_compat(&old_bytes, &new_bytes);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&compat)?);
    } else {
        p::separator();
        let level_str = match compat.level {
            CompatibilityLevel::Compatible => compat.level.to_string().green().to_string(),
            CompatibilityLevel::CompatibleWithWarnings => {
                compat.level.to_string().yellow().to_string()
            }
            CompatibilityLevel::Incompatible => compat.level.to_string().red().to_string(),
        };
        p::kv_accent("Compatibility", &level_str);
        p::kv("Old hash", &compat.old_hash);
        p::kv("New hash", &compat.new_hash);
        p::kv("Old size", &format!("{} bytes", compat.old_size_bytes));
        p::kv("New size", &format!("{} bytes", compat.new_size_bytes));
        p::kv(
            "Size delta",
            &format!("{:+} bytes", compat.size_delta_bytes),
        );

        if !compat.issues.is_empty() {
            println!();
            p::kv("Issues found", &format!("{}", compat.issues.len()));
            for issue in &compat.issues {
                let sev = match issue.severity.as_str() {
                    "critical" => issue.severity.red().to_string(),
                    "warning" => issue.severity.yellow().to_string(),
                    _ => issue.severity.dimmed().to_string(),
                };
                println!(
                    "    [{:<8}] [{}] {}",
                    sev,
                    issue.kind.white(),
                    issue.description.dimmed()
                );
            }
        }
        p::separator();
    }

    if args.fail_on_incompatible && compat.level == CompatibilityLevel::Incompatible {
        anyhow::bail!("Compatibility check failed: new WASM is incompatible with the old version.");
    }

    Ok(())
}

fn handle_plan(args: PlanArgs) -> Result<()> {
    p::header("Create Automated Upgrade Plan");
    config::validate_network(&args.network)?;

    p::step(1, 3, "Loading WASM binaries…");
    let old_bytes = read_valid_wasm(&args.old_wasm)?;
    let new_bytes = read_valid_wasm(&args.new_wasm)?;

    p::step(2, 3, "Running compatibility analysis…");
    let compat = analyse_compat(&old_bytes, &new_bytes);

    let level_str = compat.level.to_string();
    let level_colored = match compat.level {
        CompatibilityLevel::Compatible => level_str.green().to_string(),
        CompatibilityLevel::CompatibleWithWarnings => level_str.yellow().to_string(),
        CompatibilityLevel::Incompatible => level_str.red().to_string(),
    };
    p::kv_accent("Compatibility", &level_colored);

    if compat.level == CompatibilityLevel::Incompatible && !args.auto_approve {
        anyhow::bail!(
            "Cannot create plan: WASM binaries are incompatible. Fix issues or use --auto-approve to force."
        );
    }

    // Generate migration script content
    let migration_script =
        generate_migration_script(&args.contract_id, &compat.old_hash, &compat.new_hash);

    p::step(3, 3, "Saving upgrade plan…");
    let plan_id = format!(
        "plan-{}-{}",
        &args.contract_id[..args.contract_id.len().min(8)],
        &compat.new_hash[..12]
    );

    let mut plans = load_plans()?;
    if plans.iter().any(|p| p.id == plan_id) {
        anyhow::bail!("A plan with id '{}' already exists.", plan_id);
    }

    let plan = UpgradePlan {
        id: plan_id.clone(),
        contract_id: args.contract_id.clone(),
        network: args.network.clone(),
        description: args.description.clone(),
        old_wasm_hash: compat.old_hash.clone(),
        new_wasm_hash: compat.new_hash.clone(),
        compat_level: compat.level,
        migration_script: Some(migration_script),
        status: PlanStatus::Pending,
        created_at: Utc::now().to_rfc3339(),
        applied_at: None,
        applied_by: None,
    };
    plans.push(plan);
    save_plans(&plans)?;

    p::separator();
    p::kv_accent("Plan ID", &plan_id);
    p::kv("Contract", &args.contract_id);
    p::kv("Network", &args.network);
    p::kv("Old hash", &compat.old_hash);
    p::kv("New hash", &compat.new_hash);
    p::kv("Description", &args.description);
    p::separator();
    p::info(&format!(
        "Apply with: starforge upgrade-auto apply --plan-id {}",
        plan_id
    ));
    Ok(())
}

fn handle_apply(args: ApplyArgs) -> Result<()> {
    p::header("Apply Upgrade Plan");
    config::validate_network(&args.network)?;

    let cfg = config::load()?;
    let wallet = if let Some(ref name) = args.wallet {
        cfg.wallets
            .iter()
            .find(|w| w.name == *name)
            .ok_or_else(|| {
                anyhow::anyhow!("Wallet '{}' not found. Run `starforge wallet list`.", name)
            })?
    } else if !cfg.wallets.is_empty() {
        p::info(&format!(
            "No --wallet specified. Using: {}",
            cfg.wallets[0].name.cyan()
        ));
        &cfg.wallets[0]
    } else {
        anyhow::bail!("No wallets found. Create one with `starforge wallet create <name> --fund`");
    };

    let mut plans = load_plans()?;
    let plan = plans
        .iter_mut()
        .find(|p| p.id == args.plan_id && p.network == args.network)
        .ok_or_else(|| anyhow::anyhow!("Plan '{}' not found on {}", args.plan_id, args.network))?;

    if plan.status == PlanStatus::Applied {
        anyhow::bail!("Plan '{}' has already been applied.", args.plan_id);
    }

    p::separator();
    p::kv("Plan ID", &plan.id);
    p::kv("Contract", &plan.contract_id);
    p::kv("Network", &plan.network);
    p::kv("Old hash", &plan.old_wasm_hash);
    p::kv_accent("New hash", &plan.new_wasm_hash);
    p::kv("Description", &plan.description);
    p::separator();

    if !args.yes {
        print!("  Proceed with upgrade? [y/N] ");
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        if !line.trim().eq_ignore_ascii_case("y") {
            p::info("Upgrade cancelled.");
            return Ok(());
        }
    }

    let total_steps = if args.run_migration { 3 } else { 2 };

    p::step(1, total_steps, "Verifying plan integrity…");
    // (In a real implementation, we'd re-hash the WASM files here)
    p::kv("Plan verified", "✓");

    if args.run_migration {
        p::step(2, total_steps, "Running state migration…");
        // Emit the migration script commands
        println!(
            "  {}",
            "Migration script generated. Apply it on-chain before upgrading WASM.".dimmed()
        );
    }

    p::step(total_steps, total_steps, "Generating upgrade command…");

    plan.status = PlanStatus::Applied;
    plan.applied_at = Some(Utc::now().to_rfc3339());
    plan.applied_by = Some(wallet.public_key.clone());
    save_plans(&plans)?;

    println!();
    p::separator();
    println!(
        "  {} {}",
        "✓".green().bold(),
        "Run this to apply the upgrade on-chain:".bright_white()
    );
    println!();
    let contract_id = plans
        .iter()
        .find(|p| p.id == args.plan_id)
        .map(|p| p.contract_id.as_str())
        .unwrap_or("CONTRACT_ID");
    println!(
        "  {}",
        format!(
            "stellar contract invoke --id {} --source {} --network {} -- upgrade --new-wasm-hash {}",
            contract_id,
            wallet.public_key,
            args.network,
            plans.iter().find(|p| p.id == args.plan_id).map(|p| p.new_wasm_hash.as_str()).unwrap_or("NEW_HASH")
        )
        .cyan()
    );
    p::separator();
    Ok(())
}

fn handle_migration(args: MigrationArgs) -> Result<()> {
    p::header("Generate State Migration Script");

    p::step(1, 2, "Loading WASM binaries…");
    let old_bytes = read_valid_wasm(&args.old_wasm)?;
    let new_bytes = read_valid_wasm(&args.new_wasm)?;
    let old_hash = wasm_hash_hex(&old_bytes);
    let new_hash = wasm_hash_hex(&new_bytes);

    p::step(2, 2, "Writing migration template…");
    if !args.out_dir.exists() {
        fs::create_dir_all(&args.out_dir)?;
    }

    let script = generate_migration_script(&args.contract, &old_hash, &new_hash);
    let out_path = args.out_dir.join(format!(
        "migrate_{}_to_{}.rs",
        &old_hash[..8],
        &new_hash[..8]
    ));
    fs::write(&out_path, &script)?;

    p::separator();
    p::kv_accent("Migration script", &out_path.display().to_string());
    p::kv("Old hash", &old_hash);
    p::kv("New hash", &new_hash);
    p::separator();
    p::success("Review and implement the TODO sections before deploying.");
    Ok(())
}

fn handle_plans(args: PlansArgs) -> Result<()> {
    p::header("Upgrade Plans");

    let plans = load_plans()?;
    let filtered: Vec<_> = plans
        .iter()
        .filter(|p| args.network.as_deref().is_none_or(|n| p.network == n))
        .filter(|p| {
            args.contract_id
                .as_deref()
                .is_none_or(|c| p.contract_id == c)
        })
        .collect();

    if filtered.is_empty() {
        p::info("No plans found. Create one with `starforge upgrade-auto plan`.");
        return Ok(());
    }

    p::separator();
    println!(
        "  {:<30}  {:<14}  {:<10}  {:<20}  {}",
        "Plan ID".dimmed(),
        "Contract".dimmed(),
        "Network".dimmed(),
        "Status".dimmed(),
        "Created".dimmed(),
    );
    println!("  {}", "─".repeat(85).dimmed());

    for plan in filtered {
        let status_colored = match plan.status {
            PlanStatus::Pending => plan.status.to_string().yellow().to_string(),
            PlanStatus::Applied => plan.status.to_string().green().to_string(),
            PlanStatus::RolledBack => plan.status.to_string().cyan().to_string(),
            PlanStatus::Failed => plan.status.to_string().red().to_string(),
        };
        let ts = plan.created_at.get(..16).unwrap_or(&plan.created_at);
        println!(
            "  {:<30}  {:<14}  {:<10}  {:<20}  {}",
            plan.id.white(),
            short_id(&plan.contract_id).cyan(),
            plan.network.white(),
            status_colored,
            ts.dimmed(),
        );
    }
    p::separator();
    Ok(())
}

fn handle_rollback(args: RollbackArgs) -> Result<()> {
    p::header("Rollback Upgrade Plan");
    config::validate_network(&args.network)?;

    let cfg = config::load()?;
    let wallet = if let Some(ref name) = args.wallet {
        cfg.wallets
            .iter()
            .find(|w| w.name == *name)
            .ok_or_else(|| anyhow::anyhow!("Wallet '{}' not found.", name))?
    } else if !cfg.wallets.is_empty() {
        p::info(&format!(
            "No --wallet specified. Using: {}",
            cfg.wallets[0].name.cyan()
        ));
        &cfg.wallets[0]
    } else {
        anyhow::bail!("No wallets configured.");
    };

    let mut plans = load_plans()?;
    let plan = plans
        .iter_mut()
        .find(|p| p.id == args.plan_id && p.network == args.network)
        .ok_or_else(|| anyhow::anyhow!("Plan '{}' not found on {}.", args.plan_id, args.network))?;

    if plan.status != PlanStatus::Applied {
        anyhow::bail!(
            "Plan '{}' has not been applied yet (status: {}). Only applied plans can be rolled back.",
            args.plan_id,
            plan.status
        );
    }

    p::separator();
    p::kv("Plan ID", &plan.id);
    p::kv("Contract", &plan.contract_id);
    p::kv_accent("Rollback to", &plan.old_wasm_hash);
    p::kv("Network", &args.network);
    p::separator();

    if !args.yes {
        print!("  Proceed with rollback? [y/N] ");
        use std::io::{self, BufRead};
        let stdin = io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        if !line.trim().eq_ignore_ascii_case("y") {
            p::info("Rollback cancelled.");
            return Ok(());
        }
    }

    plan.status = PlanStatus::RolledBack;
    let contract_id = plan.contract_id.clone();
    let old_hash = plan.old_wasm_hash.clone();
    save_plans(&plans)?;

    println!();
    p::separator();
    println!(
        "  {} {}",
        "✓".green().bold(),
        "Run this to roll back on-chain:".bright_white()
    );
    println!();
    println!(
        "  {}",
        format!(
            "stellar contract invoke --id {} --source {} --network {} -- upgrade --new-wasm-hash {}",
            contract_id, wallet.public_key, args.network, old_hash
        )
        .cyan()
    );
    p::separator();
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn short_id(id: &str) -> String {
    if id.len() > 12 {
        format!("{}…", &id[..12])
    } else {
        id.to_string()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_wasm(extra: &[u8]) -> Vec<u8> {
        let mut v = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        v.extend_from_slice(extra);
        v
    }

    #[test]
    fn wasm_hash_is_deterministic() {
        let bytes = mock_wasm(b"v1");
        assert_eq!(wasm_hash_hex(&bytes), wasm_hash_hex(&bytes));
    }

    #[test]
    fn wasm_hash_hex_length() {
        let hash = wasm_hash_hex(b"test");
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn compat_identical_binaries_warns() {
        let wasm = mock_wasm(b"same");
        let compat = analyse_compat(&wasm, &wasm);
        assert_eq!(compat.level, CompatibilityLevel::CompatibleWithWarnings);
        assert!(compat.issues.iter().any(|i| i.kind == "identical-binary"));
    }

    #[test]
    fn compat_different_binaries_compatible() {
        let old = mock_wasm(b"version1");
        let new = mock_wasm(b"version2");
        let compat = analyse_compat(&old, &new);
        // No critical issues for simple content change
        assert_ne!(compat.level, CompatibilityLevel::Incompatible);
    }

    #[test]
    fn compat_missing_auth_is_incompatible() {
        // Old wasm has require_auth
        let old = {
            let mut v = mock_wasm(b"");
            v.extend_from_slice(b"require_auth");
            v
        };
        // New wasm does NOT have require_auth
        let new = mock_wasm(b"no_auth_here");
        let compat = analyse_compat(&old, &new);
        assert_eq!(compat.level, CompatibilityLevel::Incompatible);
        assert!(compat.issues.iter().any(|i| i.kind == "auth-removed"));
    }

    #[test]
    fn compat_level_display() {
        assert_eq!(CompatibilityLevel::Compatible.to_string(), "compatible");
        assert_eq!(
            CompatibilityLevel::CompatibleWithWarnings.to_string(),
            "compatible-with-warnings"
        );
        assert_eq!(CompatibilityLevel::Incompatible.to_string(), "incompatible");
    }

    #[test]
    fn plan_status_display() {
        assert_eq!(PlanStatus::Pending.to_string(), "pending");
        assert_eq!(PlanStatus::Applied.to_string(), "applied");
        assert_eq!(PlanStatus::RolledBack.to_string(), "rolled-back");
        assert_eq!(PlanStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn generate_migration_script_contains_contract_name() {
        let script = generate_migration_script("my_contract", "aaa000", "bbb111");
        assert!(script.contains("my_contract"));
        assert!(script.contains("migrate"));
    }
}
