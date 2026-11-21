# PipeCore

[![CI](https://github.com/alvanhan/PipeCore/actions/workflows/ci.yml/badge.svg)](https://github.com/alvanhan/PipeCore/actions/workflows/ci.yml)
[![MIT License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

PipeCore is a standalone Rust CLI for validating, importing, updating, and exporting XLSX data with PostgreSQL and Microsoft SQL Server. Each operation uses a JSON mapping and emits a structured result that any application can parse.

| Operation | What it does |
| --- | --- |
| `validate` | Checks workbook headers and values offline. |
| `import` | Inserts new rows in one transaction after validation and staging. |
| `mutate` | Applies strict updates by unique key, or upserts existing and new rows. |
| `export` | Writes a formatted workbook with one or more sheets, filters, and sorting. |
| `doctor`, `version` | Checks a connection alias or prints the executable version. |

## Install

Download the Windows x86-64 ZIP or Linux x86-64 tarball from the [v0.1.0 release](https://github.com/alvanhan/PipeCore/releases/tag/v0.1.0). Verify the archive with the release's `SHA256SUMS` file and extract it. Linux requires a Debian 12 compatible environment with glibc 2.36 and OpenSSL 3. If necessary, run `chmod +x pipecore`. The executable does not require Rust, Excel, or ODBC.

## Quick start

The examples assume an existing table. Try a [one-row template](examples/templates/) with its matching [example mapping](examples/), or adapt the workbook sheet, fields, and database table to your own data. For SQL Server, set `connection.driver` to `mssql`, `connection.dsn_env` to `PIPECORE_MSSQL_DSN`, and the table schema to the actual schema, commonly `dbo`. Keep credentials in the process environment.

```bash
export PIPECORE_PG_DSN='host=db.example.com port=5432 dbname=app user=app_user password=REPLACE_ME sslmode=require'
./pipecore version
./pipecore doctor --config examples/connections.json --connection finance_pg
./pipecore validate --schema examples/import-finance.json --input data.xlsx
./pipecore import --schema examples/import-finance.json --input data.xlsx --dry-run
./pipecore import --schema examples/import-finance.json --input data.xlsx --report import-result.json
./pipecore export --schema examples/export-finance.json --output finance.xlsx
./pipecore mutate --schema examples/bulk-update-finance.json --input updates.xlsx --dry-run
```

On Windows, use `.\pipecore.exe` and set `$env:PIPECORE_PG_DSN` in PowerShell. For the SQL Server doctor example, use `--connection finance_mssql`. To upsert, replace the strict-update mapping with [upsert-finance.json](examples/upsert-finance.json).

Input headers must match the mapping order. Store exact decimals and long integer values as Excel text. The export writer supports a flat header at a configurable row. Use the [developer guide](docs/DEVELOPER-GUIDE.md) for full mapping examples, an invoice table setup, and a Laravel integration. The [JSON Schema](schemas/pipecore.schema.json) describes the configuration structure.

## Integrate from an application

Launch the executable with an argument array and supply the DSN in the child environment. Parse stdout as JSON and handle stderr separately. See [Laravel/PHP, Go, Java, and Python examples](examples/wrappers/).

| Exit | Meaning |
| ---: | --- |
| 0 | Operation completed |
| 2 | Invalid arguments or JSON configuration |
| 3 | Workbook layout mismatch |
| 4 | Data validation failed |
| 5 | Database precheck failed |
| 6 | Database write failed or commit could not be confirmed |
| 7 | File, resource, or export failure |
| 9 | Internal error |

For a write, check `committed` in addition to the exit code. If `commit_outcome` is `unknown`, reconcile the target before retrying. `--dry-run` checks target metadata and staging without changing target rows. Target constraints and triggers still apply to a real write. The optional `--report` path stores a JSON summary. Where available, `error_report_path` points to the detailed JSONL diagnostics.

## Develop

With Docker Desktop in Linux container mode, run these commands from PowerShell:

```powershell
Copy-Item .env.example .env
# Set development passwords in .env.
./scripts/dev.ps1
./scripts/test.ps1 -Integration
./scripts/release.ps1
```

The development script provisions fixture tables. Integration tests use isolated schemas. See [Contributing](CONTRIBUTING.md), [implementation and verification](IMPLEMENTATION.md), [security](SECURITY.md), and the [changelog](CHANGELOG.md).

[MIT License](LICENSE)
