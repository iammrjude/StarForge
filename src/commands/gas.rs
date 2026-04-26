use crate::utils::{config, optimizer, print as p, profiler};
use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum GasCommands {
    /// Analyze a compiled Soroban contract for gas/cpu opportunities
    Analyze {
        /// Path to the compiled wasm
        wasm: PathBuf,
        /// Network context (used for fee heuristics)
        #[arg(long)]
        network: Option<String>,
    },
    /// Emit an "optimized" wasm (lightweight, heuristic-based)
    Optimize {
        /// Path to the input wasm
        #[arg(long)]
        target: PathBuf,
        /// Output path for optimized wasm
        #[arg(long)]
        output: PathBuf,
    },
}

pub fn handle(cmd: GasCommands) -> Result<()> {
    match cmd {
        GasCommands::Analyze { wasm, network } => analyze(wasm, network),
        GasCommands::Optimize { target, output } => optimize(target, output),
    }
}

fn analyze(wasm: PathBuf, network: Option<String>) -> Result<()> {
    config::validate_file_path(&wasm, Some("wasm"))?;

    let cfg = config::load()?;
    let network = network.unwrap_or(cfg.network);
    config::validate_network(&network)?;

    p::header("Gas Analyzer");
    p::kv("Network", &network);
    p::kv("Wasm", &wasm.display().to_string());

    let t = profiler::Timer::start();
    let report = optimizer::analyze_wasm(&wasm)?;
    let elapsed = t.elapsed();

    println!();
    p::separator();
    p::kv_accent("Size (bytes)", &report.size_bytes.to_string());
    p::kv("SHA256", &report.sha256);
    p::kv("Heuristic score", &report.score.to_string());
    if !report.suggestions.is_empty() {
        println!();
        p::info("Suggestions:");
        for s in &report.suggestions {
            println!("  - {}", s);
        }
    }
    p::separator();
    p::kv("Duration", &format!("{:?}", elapsed));
    Ok(())
}

fn optimize(target: PathBuf, output: PathBuf) -> Result<()> {
    config::validate_file_path(&target, Some("wasm"))?;

    p::header("Gas Optimizer");
    p::kv("Input", &target.display().to_string());
    p::kv("Output", &output.display().to_string());

    let t = profiler::Timer::start();
    let result = optimizer::optimize_wasm(&target, &output)?;
    let elapsed = t.elapsed();

    println!();
    p::success("Optimization output written");
    p::kv("Bytes in", &result.input_size_bytes.to_string());
    p::kv("Bytes out", &result.output_size_bytes.to_string());
    p::kv("Duration", &format!("{:?}", elapsed));
    Ok(())
}
