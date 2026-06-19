#![deny(unsafe_code)]
#![deny(missing_docs)]
#![cfg_attr(
    not(test),
    deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::indexing_slicing)
)]

//! `cargo-klauthed` — scaffolding CLI for the [klauthed](https://klauthed.github.io/rust-libraries/)
//! framework.
//!
//! Installed as a cargo subcommand (`cargo install klauthed-cli`), so:
//!
//! ```sh
//! cargo klauthed new my-service   # generates ./my-service, ready to `cargo run`
//! ```

mod scaffold;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

/// Relational backend choices for `--database`.
#[derive(Clone, Copy, ValueEnum)]
enum DbArg {
    Postgres,
    Mysql,
    Sqlite,
}

impl From<DbArg> for scaffold::Database {
    fn from(value: DbArg) -> Self {
        match value {
            DbArg::Postgres => Self::Postgres,
            DbArg::Mysql => Self::Mysql,
            DbArg::Sqlite => Self::Sqlite,
        }
    }
}

/// Scaffolding CLI for the klauthed framework.
#[derive(Parser)]
#[command(name = "cargo-klauthed", bin_name = "cargo klauthed", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scaffold a new klauthed service into a new directory.
    New {
        /// Name of the service (also the crate name): letters, digits, `-`/`_`,
        /// starting with a letter.
        name: String,
        /// Directory to create the service in (default: `./<name>`).
        #[arg(long)]
        path: Option<PathBuf>,
        /// Include JWT authentication: a `/login` endpoint and a protected
        /// `/api/me` route (enables the `security` feature).
        #[arg(long)]
        with_jwt: bool,
        /// Wire a relational connection pool into the web layer (enables the
        /// matching `klauthed` backend feature and adds a `[database]` config).
        #[arg(long, value_enum)]
        database: Option<DbArg>,
        /// Start an interval scheduler with an example recurring task (enables
        /// the `scheduler` feature).
        #[arg(long)]
        with_scheduler: bool,
    },
}

fn main() -> ExitCode {
    // When invoked as `cargo klauthed …`, cargo runs us with argv
    // `[cargo-klauthed, klauthed, …]`; drop the injected subcommand name so the
    // binary also works when run directly as `cargo-klauthed …`.
    let mut args: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if args.get(1).and_then(|a| a.to_str()) == Some("klauthed") {
        args.remove(1);
    }

    match Cli::parse_from(args).command {
        Command::New { name, path, with_jwt, database, with_scheduler } => {
            let dir = path.unwrap_or_else(|| PathBuf::from(&name));
            let options = scaffold::Options {
                with_jwt,
                database: database.map(scaffold::Database::from),
                with_scheduler,
            };
            match scaffold::scaffold(&name, &dir, &options) {
                Ok(_) => {
                    print_success(&name, &dir);
                    ExitCode::SUCCESS
                }
                Err(error) => {
                    eprintln!("error: {error}");
                    ExitCode::FAILURE
                }
            }
        }
    }
}

fn print_success(name: &str, dir: &std::path::Path) {
    let dir = dir.display();
    println!("Created klauthed service '{name}' at {dir}\n");
    println!("  cd {dir}");
    println!("  cargo run\n");
    println!("Then try:  curl localhost:8080/hello");
    println!("Docs:      https://klauthed.github.io/rust-libraries/");
}
