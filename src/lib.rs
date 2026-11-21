pub mod config;
pub mod db;
pub mod excel;
pub mod export;
pub mod report;
pub mod types;

pub static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn check_cancelled() -> anyhow::Result<()> {
    anyhow::ensure!(
        !CANCELLED.load(std::sync::atomic::Ordering::Relaxed),
        "INTERRUPTED"
    );
    Ok(())
}

use config::Config;
use report::{Detail, Report};
use std::path::Path;

pub async fn run(
    c: &Config,
    operation: &str,
    input: Option<&Path>,
    output: Option<&Path>,
    dry: bool,
    report: &mut Report,
) -> Result<(), u8> {
    report.driver = Some(c.connection.driver.clone());
    report.mode = c.mutation.as_ref().map(|m| m.mode.clone());
    report.max_samples = c.validation.max_error_samples;
    if operation != "validate" && operation != c.operation {
        record(
            report,
            "CONFIG_INVALID",
            "Schema operation does not match command",
        );
        return Err(2);
    }
    report
        .open_errors(Path::new(&c.execution.temp_dir))
        .map_err(|_| {
            record(report, "IO_FAILED", "Cannot create error report");
            7
        })?;
    if operation == "validate" && c.operation == "export" {
        record(
            report,
            "CONFIG_INVALID",
            "validate requires an input schema",
        );
        return Err(2);
    }
    let mut db = if operation == "validate" {
        None
    } else {
        Some(db::Db::connect(&c.connection).await.map_err(|_| {
            record(
                report,
                "DB_PRECHECK_FAILED",
                "Cannot connect to database; check environment, reachability and TLS",
            );
            5
        })?)
    };
    if operation == "export" {
        let path = output
            .or_else(|| c.output.as_deref().map(Path::new))
            .or_else(|| {
                c.workbook
                    .as_ref()
                    .and_then(|w| w.filename.as_deref())
                    .map(Path::new)
            })
            .ok_or_else(|| {
                record(report, "CONFIG_INVALID", "Output path required");
                2
            })?;
        return export::export(db.as_mut().unwrap(), c, path, report, dry)
            .await
            .map_err(|_| {
                record(
                    report,
                    "IO_FAILED",
                    "Export failed; inspect schema, permissions, database and output path",
                );
                7
            });
    }
    let path = input
        .or_else(|| c.input.as_deref().map(Path::new))
        .ok_or_else(|| {
            record(report, "CONFIG_INVALID", "Input path required");
            2
        })?;
    let mut meta = vec![];
    let mut keys = c
        .mutation
        .as_ref()
        .map(|m| m.matching.keys.clone())
        .unwrap_or_default();
    if let Some(db) = &mut db {
        meta = db.inspect(c.target.as_ref().unwrap()).await.map_err(|_| {
            record(report, "DB_PRECHECK_FAILED", "Target metadata unavailable");
            5
        })?;
        db::precheck(c, &meta).map_err(|_| {
            record(
                report,
                "DB_PRECHECK_FAILED",
                "Target types, required columns or writable mappings do not match schema",
            );
            5
        })?;
        let indexes = db
            .unique_keys(c.target.as_ref().unwrap())
            .await
            .map_err(|_| {
                record(report, "DB_PRECHECK_FAILED", "Cannot inspect target keys");
                5
            })?;
        if c.operation == "mutate"
            && !indexes
                .iter()
                .any(|ix| ix.len() == keys.len() && keys.iter().all(|k| ix.contains(k)))
        {
            record(report, "DB_PRECHECK_FAILED", "TARGET_KEY_NOT_UNIQUE");
            return Err(5);
        }
        if c.operation == "import" {
            keys = indexes
                .into_iter()
                .find(|k| {
                    k.iter().all(|n| {
                        c.fields.iter().any(|f| f.column == *n)
                            && meta.iter().any(|m| m.name == *n && !m.nullable)
                    })
                })
                .unwrap_or_default();
        }
    }
    let spool = excel::validate(c, path, &keys, report).map_err(|e| {
        let msg = e.to_string();
        let template = matches!(
            msg.as_str(),
            "HEADER_MISMATCH" | "SHEET_NOT_FOUND" | "MERGED_CELLS_UNSUPPORTED" | "MALFORMED_ROW"
        );
        record(
            report,
            if template {
                "TEMPLATE_MISMATCH"
            } else {
                "IO_FAILED"
            },
            if template {
                &msg
            } else {
                "Cannot read workbook or workbook exceeds safety limits"
            },
        );
        if template { 3 } else { 7 }
    })?;
    if !meta.is_empty() {
        db::check_values(c, &meta, &spool, report).map_err(|_| {
            record(report, "IO_FAILED", "Cannot read validation spool");
            7
        })?;
    }
    if report.summary.error_count > 0 {
        report.fail("DATA_VALIDATION_FAILED");
        return Err(4);
    }
    if let Some(db) = &mut db {
        let retries = c
            .mutation
            .as_ref()
            .map_or(0, |m| m.max_serialization_retries);
        for attempt in 0..=retries {
            match db::apply(db, c, &meta, &spool, &keys, report, dry).await {
                Ok(()) => break,
                Err(e) => {
                    let retry = e
                        .downcast_ref::<tokio_postgres::Error>()
                        .and_then(|e| e.code())
                        .is_some_and(|c| matches!(c.code(), "40001" | "40P01"))
                        || e.downcast_ref::<tiberius::error::Error>().is_some_and(
                            |e| matches!(e,tiberius::error::Error::Server(s) if s.code()==1205),
                        );
                    if retry && attempt < retries && report.commit_outcome != "unknown" {
                        tokio::time::sleep(std::time::Duration::from_millis(
                            100 * (attempt as u64 + 1),
                        ))
                        .await;
                        continue;
                    }
                    record(
                        report,
                        "DB_WRITE_FAILED",
                        if report.commit_outcome == "unknown" {
                            "Commit acknowledgement lost; outcome unknown. Reconcile target before retrying"
                        } else if retry {
                            "SERIALIZATION_RETRY_EXHAUSTED"
                        } else {
                            "Database rejected transaction; target changes rolled back"
                        },
                    );
                    return Err(6);
                }
            }
        }
        if report.summary.error_count > 0 {
            report.fail("DB_PRECHECK_FAILED");
            return Err(5);
        }
    }
    Ok(())
}
pub fn record(report: &mut Report, code: &str, message: &str) {
    report.fail(code);
    if report.error(Detail::new(code, message)).is_err() {
        report.fail("IO_FAILED");
    }
}
