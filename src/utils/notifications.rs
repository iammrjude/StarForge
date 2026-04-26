use colored::*;

pub fn info(message: &str) {
    println!("  {} {}", "•".bright_blue(), message);
}

pub fn success(message: &str) {
    println!("  {} {}", "✓".green().bold(), message);
}

pub fn warn(message: &str) {
    eprintln!("  {} {}", "!".yellow().bold(), message);
}

