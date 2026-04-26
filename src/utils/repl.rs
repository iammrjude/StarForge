use anyhow::Result;
use colored::*;
use std::io::{self, Write};

pub struct Repl<R>
where
    R: ReplRunner,
{
    runner: R,
}

pub trait ReplRunner {
    fn run_invocation(&mut self, function: &str, args: &[String]) -> Result<String>;
}

impl<R> Repl<R>
where
    R: ReplRunner,
{
    pub fn new(runner: R) -> Self {
        Self { runner }
    }

    pub fn run(mut self) -> Result<()> {
        println!(
            "  {} {}",
            "StarForge Shell".bright_cyan().bold(),
            "(type :help for commands)".dimmed()
        );

        let stdin = io::stdin();
        let mut buffer = String::new();

        loop {
            buffer.clear();
            print!("{}", "> ".bright_green().bold());
            io::stdout().flush()?;

            if stdin.read_line(&mut buffer)? == 0 {
                break;
            }

            let line = buffer.trim();
            if line.is_empty() {
                continue;
            }

            if line == ":q" || line == ":quit" || line == ":exit" {
                break;
            }

            if line == ":help" {
                println!("  {}", "Commands:".bold());
                println!("    :help              Show help");
                println!("    :quit | :exit      Exit shell");
                println!("    fn(arg1, arg2)     Invoke a contract function");
                continue;
            }

            let (function, args) = parse_invocation(line)?;
            match self.runner.run_invocation(&function, &args) {
                Ok(out) => println!("{}", out),
                Err(e) => eprintln!("  {} {}", "✗".red().bold(), e),
            }
        }

        Ok(())
    }
}

fn parse_invocation(input: &str) -> Result<(String, Vec<String>)> {
    let open = input
        .find('(')
        .ok_or_else(|| anyhow::anyhow!("Expected invocation like fn(\"arg\")"))?;
    let close = input
        .rfind(')')
        .ok_or_else(|| anyhow::anyhow!("Missing closing ')'"))?;
    if close < open {
        anyhow::bail!("Invalid invocation");
    }

    let function = input[..open].trim();
    if function.is_empty() {
        anyhow::bail!("Missing function name");
    }

    let args_raw = input[open + 1..close].trim();
    let args = split_args(args_raw)?;
    Ok((function.to_string(), args))
}

fn split_args(input: &str) -> Result<Vec<String>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }

    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '\0';
    let mut escape = false;

    for ch in input.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }

        if ch == '\\' {
            escape = true;
            continue;
        }

        if in_quotes {
            if ch == quote_char {
                in_quotes = false;
                continue;
            }
            current.push(ch);
            continue;
        }

        if ch == '"' || ch == '\'' {
            in_quotes = true;
            quote_char = ch;
            continue;
        }

        if ch == ',' {
            args.push(current.trim().to_string());
            current.clear();
            continue;
        }

        current.push(ch);
    }

    if in_quotes {
        anyhow::bail!("Unclosed quote in arguments");
    }

    if escape {
        anyhow::bail!("Trailing escape in arguments");
    }

    args.push(current.trim().to_string());
    Ok(args)
}

