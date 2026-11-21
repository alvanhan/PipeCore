use clap::{Args, Parser, Subcommand};
use pipecore::{
    config::{Config, Connection},
    report::Report,
};
use std::{path::PathBuf, time::Instant};
#[derive(Parser)]
#[command(
    name = "pipecore",
    about = "Atomic XLSX import, export and bulk mutation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Version,
    Doctor {
        #[arg(long)]
        config: PathBuf,
        #[arg(long)]
        connection: String,
    },
    Validate(Job),
    Import(Job),
    Export(Job),
    Mutate(Job),
}
#[derive(Args)]
struct Job {
    #[arg(long)]
    schema: PathBuf,
    #[arg(long)]
    input: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long)]
    dry_run: bool,
}
#[tokio::main]
async fn main() {
    std::panic::set_hook(Box::new(|_| {}));
    tokio::spawn(async {
        #[cfg(unix)]
        {
            if let Ok(mut term) =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            {
                tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
            }
        }
        #[cfg(not(unix))]
        {
            let _ = tokio::signal::ctrl_c().await;
        }
        pipecore::CANCELLED.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    use futures_util::FutureExt;
    if std::panic::AssertUnwindSafe(run_cli())
        .catch_unwind()
        .await
        .is_err()
    {
        let mut r = Report::new("unknown");
        pipecore::record(
            &mut r,
            "INTERNAL_ERROR",
            "Unexpected internal error; reconcile target if interruption occurred during commit",
        );
        println!("{}", serde_json::to_string(&r).unwrap());
        std::process::exit(9);
    }
}
async fn run_cli() {
    let started = Instant::now();
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let help = matches!(
                e.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            );
            if help {
                eprintln!("{e}");
                println!("{{\"status\":\"SUCCESS\",\"code\":\"HELP\"}}");
                return;
            }
            let mut r = Report::new("unknown");
            pipecore::record(
                &mut r,
                "CONFIG_INVALID",
                "Invalid CLI arguments; use --help",
            );
            println!("{}", serde_json::to_string(&r).unwrap());
            std::process::exit(2);
        }
    };
    let (op, job) = match cli.command {
        Command::Version => {
            println!(
                "{{\"status\":\"SUCCESS\",\"code\":\"SUCCESS\",\"version\":\"{}\"}}",
                env!("CARGO_PKG_VERSION")
            );
            return;
        }
        Command::Doctor { config, connection } => {
            let result = async {
                let map: std::collections::HashMap<String, Connection> =
                    serde_json::from_reader(std::fs::File::open(config)?)?;
                let conn = map
                    .get(&connection)
                    .ok_or_else(|| anyhow::anyhow!("Unknown connection alias"))?;
                anyhow::ensure!(
                    matches!(conn.driver.as_str(), "postgres" | "mssql"),
                    "Invalid driver"
                );
                let mut db = pipecore::db::Db::connect(conn).await?;
                db.scalar(&format!("SELECT {}", db.text("1"))).await?;
                Ok::<(), anyhow::Error>(())
            }
            .await;
            let mut r = Report::new("doctor");
            let exit = if result.is_err() {
                pipecore::record(
                    &mut r,
                    "DB_PRECHECK_FAILED",
                    "Connection/config check failed",
                );
                5
            } else {
                0
            };
            r.elapsed_ms = started.elapsed().as_millis();
            println!("{}", serde_json::to_string(&r).unwrap());
            std::process::exit(exit);
        }
        Command::Validate(j) => ("validate", j),
        Command::Import(j) => ("import", j),
        Command::Export(j) => ("export", j),
        Command::Mutate(j) => ("mutate", j),
    };
    let mut report = Report::new(op);
    // Open the report destination before any target write.
    let mut report_file = match &job.report {
        Some(p) => match tempfile::NamedTempFile::new_in(
            p.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(std::path::Path::new(".")),
        ) {
            Ok(f) => Some(f),
            Err(_) => {
                pipecore::record(
                    &mut report,
                    "IO_FAILED",
                    "Cannot prepare report destination",
                );
                println!("{}", serde_json::to_string(&report).unwrap());
                std::process::exit(7);
            }
        },
        None => None,
    };
    let mut exit = match Config::read(&job.schema) {
        Ok(c) => pipecore::run(
            &c,
            op,
            job.input.as_deref(),
            job.output.as_deref(),
            job.dry_run,
            &mut report,
        )
        .await
        .err()
        .unwrap_or(0),
        Err(_) => {
            pipecore::record(
                &mut report,
                "CONFIG_INVALID",
                "Schema is unreadable, malformed or violates supported contract",
            );
            2
        }
    };
    report.elapsed_ms = started.elapsed().as_millis();
    if report.finish().is_err() {
        report.fail("IO_FAILED");
        exit = 7;
    }
    if let Some(file) = &mut report_file
        && (serde_json::to_writer_pretty(file.as_file_mut(), &report).is_err()
            || file.as_file().sync_all().is_err())
    {
        report.fail("IO_FAILED");
        exit = 7;
    }
    if let Some(file) = report_file
        && file.persist(job.report.as_ref().unwrap()).is_err()
    {
        report.fail("IO_FAILED");
        exit = 7;
    }
    println!("{}", serde_json::to_string(&report).unwrap());
    std::process::exit(exit as i32);
}
