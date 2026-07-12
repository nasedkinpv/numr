//! numr-cli - Command-line calculator
//!
//! Usage:
//!   numr-cli "300$ in rub"           # Single expression
//!   echo "100 + 50" | numr-cli       # Pipe mode
//!   numr-cli -f calculations.txt     # File mode
//!   numr-cli -i                      # Interactive REPL
//!   numr-cli --server                # JSON-RPC server mode

use std::io::{self, BufRead, IsTerminal, Write};
use std::path::PathBuf;

use clap::{CommandFactory, Parser};
use numr_core::{Engine, Value};

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

    /// JSON-RPC server mode (reads from stdin, writes to stdout)
    #[arg(long)]
    server: bool,

    /// Show aligned "input = result" output (default for -f file mode)
    #[arg(short, long)]
    verbose: bool,

    /// Show running total
    #[arg(short, long)]
    total: bool,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    let mut engine = Engine::new();
    let cache_loaded = match engine.load_rates_from_cache() {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("Warning: failed to load the exchange-rate cache: {error}");
            false
        }
    };

    // Server mode uses deterministic defaults plus an explicitly loaded cache. Clients can request
    // a network refresh through `reload_rates` when they need one.
    if args.server {
        numr_cli::server::run_server(&mut engine)?;
        return Ok(());
    }

    // Fetch fresh rates if the explicit cache load found no usable entry.
    if !cache_loaded {
        let rt = tokio::runtime::Runtime::new()?;
        match rt.block_on(numr_core::fetch_rates()) {
            Ok(result) => match engine.apply_raw_rates(&result.rates) {
                Ok(_) => {
                    if let Err(error) = engine.save_rates_to_cache(&result.rates) {
                        eprintln!("Warning: failed to persist the exchange-rate cache: {error}");
                    }
                    if let Some(warning) = result.warning {
                        eprintln!("Warning: {warning}");
                    }
                }
                Err(error) => eprintln!("Warning: rejected exchange rates: {error}"),
            },
            Err(e) => eprintln!("Warning: {e}"),
        }
    }

    // Determine input source
    // File mode defaults to verbose (aligned output), everything else to quiet
    if let Some(expr) = &args.expression {
        // Single expression mode
        eval_and_print(&mut engine, expr, !args.verbose);
    } else if let Some(path) = &args.file {
        // File mode — verbose by default
        let content = std::fs::read_to_string(path)?;
        let document = engine.evaluate_document(&content);
        for line in document.lines {
            print_evaluated(&line.input, &line.value, false);
        }
    } else if args.interactive {
        // Interactive REPL
        run_repl(&mut engine)?;
    } else if !io::stdin().is_terminal() {
        // Pipe mode (stdin is not a tty)
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let line = line?;
            eval_and_print(&mut engine, &line, !args.verbose);
        }
    } else {
        Args::command().print_help()?;
        println!();
        std::process::exit(1);
    }

    // Show total if requested
    if args.total {
        let sum = engine.sum();
        println!("─────────────");
        println!("Total: {sum}");
    }

    Ok(())
}

fn eval_and_print(engine: &mut Engine, input: &str, quiet: bool) {
    let result = engine.eval(input);
    print_evaluated(input, &result, quiet);
}

fn print_evaluated(input: &str, result: &Value, quiet: bool) {
    if quiet {
        if !result.is_empty() {
            println!("{result}");
        }
    } else {
        let result_str = result.to_string();
        if result_str.is_empty() {
            println!("{input}");
        } else {
            // Pad input to align results
            let padding = 40usize.saturating_sub(input.len());
            println!("{}{:>width$} = {}", input, "", result_str, width = padding);
        }
    }
}

fn run_repl(engine: &mut Engine) -> io::Result<()> {
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

        eval_and_print(engine, line, true);
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
