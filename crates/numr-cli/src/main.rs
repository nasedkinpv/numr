//! numr-cli - Command-line calculator
//!
//! Usage:
//!   numr-cli "300$ in rub"           # Single expression
//!   echo "100 + 50" | numr-cli       # Pipe mode
//!   numr-cli -f calculations.txt     # File mode
//!   numr-cli -i                      # Interactive REPL

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use numr_core::Engine;

#[derive(Parser, Debug)]
#[command(name = "numr-cli")]
#[command(about = "A natural language calculator", long_about = None)]
struct Args {
    /// Expression to evaluate
    expression: Option<String>,

    /// Read expressions from file
    #[arg(short, long, value_name = "FILE")]
    file: Option<PathBuf>,

    /// Interactive REPL mode
    #[arg(short, long)]
    interactive: bool,

    /// Show only the final result (no input echo)
    #[arg(short, long)]
    quiet: bool,

    /// Show running total
    #[arg(short, long)]
    total: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut engine = Engine::new();

    // Determine input source
    if let Some(expr) = &args.expression {
        // Single expression mode
        eval_and_print(&mut engine, expr, args.quiet);
    } else if let Some(path) = &args.file {
        // File mode
        let content = std::fs::read_to_string(path)?;
        for line in content.lines() {
            eval_and_print(&mut engine, line, args.quiet);
        }
    } else if args.interactive {
        // Interactive REPL
        run_repl(&mut engine, args.quiet)?;
    } else if !atty::is(atty::Stream::Stdin) {
        // Pipe mode (stdin is not a tty)
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line?;
            eval_and_print(&mut engine, &line, args.quiet);
        }
    } else {
        // No input, show help
        eprintln!("Usage: numr-cli <expression>");
        eprintln!("       numr-cli -f <file>");
        eprintln!("       numr-cli -i");
        eprintln!("       echo \"100 + 50\" | numr-cli");
        std::process::exit(1);
    }

    // Show total if requested
    if args.total {
        let sum = engine.sum();
        println!("─────────────");
        println!("Total: {}", sum);
    }

    Ok(())
}

fn eval_and_print(engine: &mut Engine, input: &str, quiet: bool) {
    let result = engine.eval(input);

    if quiet {
        if !result.is_empty() {
            println!("{}", result);
        }
    } else {
        let result_str = result.to_string();
        if result_str.is_empty() {
            println!("{}", input);
        } else {
            // Pad input to align results
            let padding = 40usize.saturating_sub(input.len());
            println!("{}{:>width$} = {}", input, "", result_str, width = padding);
        }
    }
}

fn run_repl(engine: &mut Engine, quiet: bool) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    println!("numr - Natural Language Calculator");
    println!("Type expressions to calculate. Press Ctrl+D to exit.\n");

    loop {
        print!("> ");
        stdout.flush()?;

        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            // EOF
            println!();
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Special commands
        match line.to_lowercase().as_str() {
            "quit" | "exit" => break,
            "clear" => {
                engine.clear();
                println!("Cleared.");
                continue;
            }
            "total" | "sum" => {
                println!("Total: {}", engine.sum());
                continue;
            }
            "help" => {
                print_help();
                continue;
            }
            _ => {}
        }

        eval_and_print(engine, line, quiet);
    }

    Ok(())
}

fn print_help() {
    println!(
        r#"
Commands:
  help     Show this help
  clear    Clear all variables and history
  total    Show sum of all results
  quit     Exit the REPL

Examples:
  10 + 20              Basic arithmetic
  20% of 150           Percentage calculation
  tax = 15%            Variable assignment
  100 + tax            Use variable
  $100 in eur          Currency conversion
  2 hours + 30 min     Unit arithmetic
  2 km in miles        Unit conversion
"#
    );
}
