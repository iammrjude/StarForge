use crate::utils::{print as p, repl, sandbox::LocalSorobanSandbox};
use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct ShellArgs {
    /// Path to the compiled contract .wasm (local sandbox execution)
    #[arg(long)]
    pub contract: String,
}

pub fn handle(args: ShellArgs) -> Result<()> {
    p::header("Interactive Contract Shell");
    p::separator();
    p::kv("Contract WASM", &args.contract);
    p::separator();
    println!();

    let sandbox = LocalSorobanSandbox::new(&args.contract)?;
    let runner = ShellRunner { sandbox };
    repl::Repl::new(runner).run()
}

struct ShellRunner {
    sandbox: LocalSorobanSandbox,
}

impl repl::ReplRunner for ShellRunner {
    fn run_invocation(&mut self, function: &str, args: &[String]) -> Result<String> {
        self.sandbox.invoke(function, args)
    }
}

