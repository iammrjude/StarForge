use crate::utils::network_sim::{
    builtin_scenarios, load_scenario, save_scenario, sim_data_dir, FailureMode, NetworkSimulator,
};
use crate::utils::print as p;
use anyhow::Result;
use clap::{Args, Subcommand};
use colored::*;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum SimulateCommands {
    /// Start an interactive simulation session
    Run(RunArgs),
    /// Save current simulator state to a snapshot file
    Snapshot(SnapshotArgs),
    /// Restore simulator state from a snapshot file
    Restore(RestoreArgs),
    /// Advance virtual time in the simulator
    Time(TimeArgs),
    /// Configure failure injection for testing
    Fail(FailArgs),
    /// Run a predefined or custom simulation scenario
    Scenario(ScenarioArgs),
    /// List built-in simulation scenarios
    List,
}

#[derive(Args)]
pub struct RunArgs {
    /// Deterministic seed for reproducible execution
    #[arg(long, default_value = "42")]
    pub seed: u64,
    /// WASM hash to deploy in the simulation
    #[arg(long)]
    pub wasm_hash: Option<String>,
    /// Contract function to invoke after deploy
    #[arg(long)]
    pub invoke: Option<String>,
    /// Arguments for the invoke call
    #[arg(long, num_args = 0..)]
    pub args: Vec<String>,
    /// Simulated network latency in milliseconds
    #[arg(long, default_value = "0")]
    pub latency_ms: u64,
}

#[derive(Args)]
pub struct SnapshotArgs {
    /// Snapshot name
    #[arg(long)]
    pub name: String,
    /// Path to save snapshot (defaults to ~/.starforge/sim/<name>.json)
    #[arg(long)]
    pub path: Option<PathBuf>,
    /// Seed used when creating the snapshot state
    #[arg(long, default_value = "42")]
    pub seed: u64,
}

#[derive(Args)]
pub struct RestoreArgs {
    /// Path to snapshot file
    #[arg(long)]
    pub path: PathBuf,
    /// Seed for deterministic execution after restore
    #[arg(long, default_value = "42")]
    pub seed: u64,
}

#[derive(Args)]
pub struct TimeArgs {
    /// Advance virtual time by this many seconds
    #[arg(long, group = "advance")]
    pub seconds: Option<u64>,
    /// Advance ledger sequence by this many ledgers
    #[arg(long, group = "advance")]
    pub ledgers: Option<u32>,
    /// Seed for the simulator
    #[arg(long, default_value = "42")]
    pub seed: u64,
}

#[derive(Args)]
pub struct FailArgs {
    /// Failure mode: none, timeout, error, insufficient_fee, contract_not_found, random
    #[arg(long, default_value = "none")]
    pub mode: String,
    /// Probability (0-100) for random failure mode
    #[arg(long, default_value = "50")]
    pub probability: u8,
    /// Seed for the simulator
    #[arg(long, default_value = "42")]
    pub seed: u64,
}

#[derive(Args)]
pub struct ScenarioArgs {
    /// Built-in scenario name or path to custom scenario JSON
    #[arg(long)]
    pub name: Option<String>,
    /// Path to custom scenario file
    #[arg(long)]
    pub file: Option<PathBuf>,
    /// Save scenario result as JSON
    #[arg(long)]
    pub json: bool,
    /// Export a built-in scenario to a file
    #[arg(long)]
    pub export: Option<PathBuf>,
}

pub async fn handle(cmd: SimulateCommands) -> Result<()> {
    match cmd {
        SimulateCommands::Run(args) => handle_run(args),
        SimulateCommands::Snapshot(args) => handle_snapshot(args),
        SimulateCommands::Restore(args) => handle_restore(args),
        SimulateCommands::Time(args) => handle_time(args),
        SimulateCommands::Fail(args) => handle_fail(args),
        SimulateCommands::Scenario(args) => handle_scenario(args),
        SimulateCommands::List => handle_list(),
    }
}

fn handle_run(args: RunArgs) -> Result<()> {
    p::header("Network Simulation");
    p::kv("Seed", &args.seed.to_string());
    p::kv("Latency", &format!("{}ms", args.latency_ms));

    let mut sim = NetworkSimulator::new(args.seed);
    sim.set_latency(args.latency_ms);

    if let Some(ref hash) = args.wasm_hash {
        p::step(1, 2, "Deploying contract in simulator…");
        let contract_id = sim.deploy_contract(hash)?;
        p::success(&format!("Deployed: {}", contract_id));
        p::kv("WASM hash", hash);
        p::kv("Ledger", &sim.state().ledger_sequence.to_string());

        if let Some(ref function) = args.invoke {
            p::step(2, 2, &format!("Invoking {}…", function));
            let result = sim.invoke(&contract_id, function, &args.args)?;
            p::success("Invocation complete");
            p::kv("Return value", &result.return_value);
            p::kv("Fee (stroops)", &result.fee.to_string());
            p::kv("Ledger", &result.ledger_sequence.to_string());
            for event in &result.events {
                println!("  {} {}", "event:".dimmed(), event);
            }
        }
    } else {
        p::info("No --wasm-hash provided. Simulator initialized with empty state.");
        p::info("Use `starforge simulate scenario` to run predefined test scenarios.");
    }

    let state_path = sim_data_dir().join(format!("state_{}.json", args.seed));
    sim.save_to_file(&state_path)?;
    p::kv("State saved", &state_path.display().to_string());
    Ok(())
}

fn handle_snapshot(args: SnapshotArgs) -> Result<()> {
    p::header("Simulation Snapshot");
    let path = args
        .path
        .unwrap_or_else(|| sim_data_dir().join(format!("{}.json", args.name)));

    let mut sim = NetworkSimulator::new(args.seed);
    sim.snapshot(&args.name);
    sim.save_to_file(&path)?;

    p::success(&format!("Snapshot '{}' saved to {}", args.name, path.display()));
    Ok(())
}

fn handle_restore(args: RestoreArgs) -> Result<()> {
    p::header("Restore Simulation State");
    let sim = NetworkSimulator::load_from_file(&args.path, args.seed)?;

    p::kv("Ledger sequence", &sim.state().ledger_sequence.to_string());
    p::kv("Contracts", &sim.state().contracts.len().to_string());
    p::kv("Accounts", &sim.state().accounts.len().to_string());
    p::kv("Events", &sim.state().events.len().to_string());
    p::success("State restored successfully");
    Ok(())
}

fn handle_time(args: TimeArgs) -> Result<()> {
    p::header("Simulation Time Control");
    let mut sim = NetworkSimulator::new(args.seed);

    if let Some(seconds) = args.seconds {
        sim.advance_time(seconds);
        p::success(&format!("Advanced virtual time by {} seconds", seconds));
        p::kv("Timestamp", &sim.state().timestamp.to_string());
    } else if let Some(ledgers) = args.ledgers {
        sim.advance_ledger(ledgers);
        p::success(&format!("Advanced ledger by {} sequences", ledgers));
        p::kv("Ledger sequence", &sim.state().ledger_sequence.to_string());
    } else {
        anyhow::bail!("Specify --seconds or --ledgers to advance time");
    }
    Ok(())
}

fn handle_fail(args: FailArgs) -> Result<()> {
    p::header("Failure Injection");
    let mode = parse_failure_mode(&args.mode, args.probability)?;
    let mut sim = NetworkSimulator::new(args.seed);
    sim.set_failure_mode(mode.clone());

    p::kv("Mode", &format!("{:?}", mode));
    p::kv("Seed", &args.seed.to_string());

    match sim.deploy_contract("test_hash") {
        Ok(id) => {
            p::success(&format!("Operation succeeded despite mode (deployed {})", id));
        }
        Err(e) => {
            p::warn(&format!("Injected failure triggered: {}", e));
        }
    }
    Ok(())
}

fn handle_scenario(args: ScenarioArgs) -> Result<()> {
    if let Some(export_path) = args.export {
        let scenarios = builtin_scenarios();
        let name = args
            .name
            .as_deref()
            .unwrap_or("basic-deploy-invoke");
        let scenario = scenarios
            .iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow::anyhow!("Built-in scenario '{}' not found", name))?;
        save_scenario(scenario, &export_path)?;
        p::success(&format!("Exported scenario to {}", export_path.display()));
        return Ok(());
    }

    p::header("Simulation Scenario");

    let scenario = if let Some(ref file) = args.file {
        load_scenario(file)?
    } else {
        let name = args
            .name
            .as_deref()
            .unwrap_or("basic-deploy-invoke");
        builtin_scenarios()
            .into_iter()
            .find(|s| s.name == name)
            .ok_or_else(|| anyhow::anyhow!("Built-in scenario '{}' not found", name))?
    };

    p::kv("Scenario", &scenario.name);
    p::kv("Description", &scenario.description);
    p::kv("Steps", &scenario.steps.len().to_string());
    p::kv("Seed", &scenario.seed.to_string());
    println!();

    let mut sim = NetworkSimulator::new(scenario.seed);
    let result = sim.run_scenario(&scenario);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if result.passed {
        p::success(&format!(
            "Scenario passed ({}/{} steps)",
            result.steps_run, result.steps_total
        ));
    } else {
        p::warn(&format!(
            "Scenario failed at step {}/{}",
            result.steps_run, result.steps_total
        ));
        for err in &result.errors {
            println!("  {} {}", "✗".red(), err);
        }
    }
    p::kv("Final ledger", &result.final_ledger.to_string());
    Ok(())
}

fn handle_list() -> Result<()> {
    p::header("Built-in Simulation Scenarios");
    for scenario in builtin_scenarios() {
        println!(
            "  {} {} — {}",
            "•".cyan(),
            scenario.name.bright_white(),
            scenario.description.dimmed()
        );
        println!(
            "    {} steps, seed={}",
            scenario.steps.len(),
            scenario.seed
        );
    }
    Ok(())
}

fn parse_failure_mode(mode: &str, probability: u8) -> Result<FailureMode> {
    match mode {
        "none" => Ok(FailureMode::None),
        "timeout" => Ok(FailureMode::RpcTimeout),
        "error" => Ok(FailureMode::RpcError),
        "insufficient_fee" => Ok(FailureMode::InsufficientFee),
        "contract_not_found" => Ok(FailureMode::ContractNotFound),
        "random" => Ok(FailureMode::Random {
            probability_pct: probability.min(100),
        }),
        other => anyhow::bail!(
            "Unknown failure mode '{}'. Use: none, timeout, error, insufficient_fee, contract_not_found, random",
            other
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_failure_modes() {
        assert!(matches!(
            parse_failure_mode("none", 0).unwrap(),
            FailureMode::None
        ));
        assert!(matches!(
            parse_failure_mode("random", 30).unwrap(),
            FailureMode::Random { probability_pct: 30 }
        ));
    }
}
