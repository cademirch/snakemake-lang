//! CLI entry point for snakemake-lang.
//!
//! Provides standalone commands for parsing and compiling Snakefiles.
//! Useful for debugging, testing, and integration with other tools.
//!
//!     snakemake-lang compile Snakefile    # emit virtual Python
//!     snakemake-lang parse Snakefile      # emit AST as JSON
//!     snakemake-lang check Snakefile      # parse and report errors

#[cfg(feature = "cli")]
use clap::{Parser, Subcommand};

#[cfg(feature = "cli")]
#[derive(Parser)]
#[command(name = "snakemake-lang", about = "Snakemake language tools")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[cfg(feature = "cli")]
#[derive(Subcommand)]
enum Command {
    /// Compile a Snakefile to virtual Python.
    Compile {
        /// Path to the Snakefile.
        path: String,
        /// Also emit the source map as JSON.
        #[arg(long)]
        source_map: bool,
    },
    /// Parse a Snakefile and emit the AST as JSON.
    Parse {
        /// Path to the Snakefile.
        path: String,
    },
    /// Parse a Snakefile and report any errors.
    Check {
        /// Paths to check.
        paths: Vec<String>,
    },
}

#[cfg(feature = "cli")]
fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Compile { path, source_map } => {
            let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                eprintln!("Error reading {}: {}", path, e);
                std::process::exit(1);
            });

            match snakemake_lang::compile(&source, &path) {
                Ok(result) => {
                    print!("{}", result.python);
                    if source_map {
                        let map = result.source_map.to_linemap(&result.python, &source);
                        eprintln!(
                            "{}",
                            serde_json::to_string_pretty(&map).unwrap()
                        );
                    }
                }
                Err(errors) => {
                    for err in &errors {
                        eprintln!(
                            "{}:{}:{}: {}",
                            path, err.line, err.column, err.message
                        );
                    }
                    std::process::exit(1);
                }
            }
        }

        Command::Parse { path } => {
            let source = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                eprintln!("Error reading {}: {}", path, e);
                std::process::exit(1);
            });

            match snakemake_lang::parse(&source, &path) {
                Ok(ast) => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&ast).unwrap()
                    );
                }
                Err(errors) => {
                    for err in &errors {
                        eprintln!(
                            "{}:{}:{}: {}",
                            path, err.line, err.column, err.message
                        );
                    }
                    std::process::exit(1);
                }
            }
        }

        Command::Check { paths } => {
            let mut has_errors = false;

            for path in &paths {
                let source = match std::fs::read_to_string(path) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("Error reading {}: {}", path, e);
                        has_errors = true;
                        continue;
                    }
                };

                match snakemake_lang::parse(&source, path) {
                    Ok(_) => {
                        println!("{}: OK", path);
                    }
                    Err(errors) => {
                        has_errors = true;
                        for err in &errors {
                            eprintln!(
                                "{}:{}:{}: {}",
                                path, err.line, err.column, err.message
                            );
                        }
                    }
                }
            }

            if has_errors {
                std::process::exit(1);
            }
        }
    }
}

#[cfg(not(feature = "cli"))]
fn main() {
    eprintln!("CLI not enabled. Build with: cargo build --features cli");
    std::process::exit(1);
}
