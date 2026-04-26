use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct LocalSorobanSandbox {
    wasm_path: PathBuf,
}

impl LocalSorobanSandbox {
    pub fn new<P: AsRef<Path>>(wasm_path: P) -> Result<Self> {
        let wasm_path = wasm_path.as_ref().to_path_buf();
        if !wasm_path.exists() {
            anyhow::bail!("Contract wasm not found: {}", wasm_path.display());
        }
        Ok(Self { wasm_path })
    }

    pub fn invoke(&self, function: &str, args: &[String]) -> Result<String> {
        // Best-effort local execution via Stellar CLI / Soroban CLI.
        // The exact flags can vary by CLI version; we keep this minimal and transparent.
        let mut cmd = Command::new("stellar");
        cmd.arg("contract")
            .arg("invoke")
            .arg("--wasm")
            .arg(&self.wasm_path)
            .arg("--fn")
            .arg(function);

        if !args.is_empty() {
            cmd.arg("--");
            for arg in args {
                cmd.arg(arg);
            }
        }

        let out = cmd
            .output()
            .with_context(|| "Failed to run `stellar contract invoke` (is `stellar` installed?)")?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            anyhow::bail!(
                "Local invoke failed.\nstdout:\n{}\nstderr:\n{}",
                stdout.trim(),
                stderr.trim()
            );
        }

        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

