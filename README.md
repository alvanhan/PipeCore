# PipeCore

[![Latest release](https://img.shields.io/github/v/release/alvanhan/PipeCore?logo=github)](https://github.com/alvanhan/PipeCore/releases/latest)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-000000?logo=rust)](Cargo.toml)
[![Platforms: Windows and Linux](https://img.shields.io/badge/platform-Windows%20%7C%20Linux-2563eb)](#install)
[![XLSX import and export](https://img.shields.io/badge/Excel-XLSX-217346)](#quick-start)
[![PostgreSQL and SQL Server](https://img.shields.io/badge/databases-PostgreSQL%20%7C%20SQL%20Server-336791)](docs/DEVELOPER-GUIDE.md)
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

Download the Windows x86-64 ZIP or Linux x86-64 tarball from the [latest release](https://github.com/alvanhan/PipeCore/releases/latest). Verify the archive with the release's `SHA256SUMS` file and extract it. Linux requires a Debian 12 compatible environment with glibc 2.36 and OpenSSL 3. If necessary, run `chmod +x pipecore`. The executable does not require Rust, Excel, or ODBC.

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

Input headers must match the mapping order. Store exact decimals and long integer values as Excel text. For export, `header.row` positions the column labels, and `header.layout` adds optional colored cells and merged ranges above them. See the [portable examples](examples/portable/) for both drivers and the [JSON Schema](schemas/pipecore.schema.json) for configuration fields.

- Insert a workbook-only column at any position in `sheets[].columns`. Supply `excel_header`, `width`, and `format` as usual and use `"value": null` for blank cells or a string, number, or boolean for a fixed value. Use `db_column` for database-backed columns.
- Set `workbook.split_files` to `true` to package split output as `part-001.xlsx`, `part-002.xlsx`, and so on. The XLSX row limit includes header rows. Pass an XLSX path to `--output`. PipeCore uses that path for single-file output and changes the extension to `.zip` when a split is needed. Read `output_format` from the JSON response.
- To replace a subset on import, set `replace_scope` to non-null equality filters, for example `[{"column":"report_date","op":"eq","value":"2026-11-30"}]`. Each scope column must be a required input field. PipeCore verifies the workbook is nonempty and in scope, then deletes and inserts in one transaction. The report includes `summary.deleted_rows`.
- SQL Server `datetime` and `smalldatetime` are mapped as logical `datetime`. Imports reject out-of-range values and values that would round in the target type.
- `validate --output rows.ndjson` can save validated rows for application-side business rules. Each line contains `sheet`, `row`, and `values` in mapping order. Create the output parent in private storage and remove the file after processing. PipeCore publishes the file only after successful validation.

## Integrate from an application

Launch the executable with an argument array and supply the DSN in the child environment. Parse stdout as JSON and handle stderr separately. See [Laravel/PHP, Go, Java, and Python examples](examples/wrappers/).

Use the [developer guide](docs/DEVELOPER-GUIDE.md) for Laravel integration and the [agent integration guide](AGENT/AGENT.MD) for a complete mapping and deployment reference. To hand off an integration, tell your coding agent to read `AGENT/AGENT.MD` first.

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

## Benchmark

PipeCore **v0.1.1** processed a **500,000-row, 25-column** synthetic financial workbook (98.0 MiB). Import with a confirmed commit took **26.24 s** on PostgreSQL (19,056 rows/s) and **47.09 s** on SQL Server (10,618 rows/s).

The Windows x86-64 release binary ran on Windows 11 with an Intel Core i5-11400H (6 cores, 12 threads) and 15.77 GiB of RAM. PostgreSQL 17.12 and SQL Server 2022 Developer (16.0.4325.4) ran in local Docker containers, with 4 CPUs and 3.83 GiB of RAM available to Docker. The CLI connected over loopback.

Each scenario was measured once per database, sequentially. Time comes from the CLI report's `elapsed_ms` and includes database calls. Memory is the observed Windows peak working set of the PipeCore process. Browser upload time, application overhead, and database and Docker memory are outside the measurement.

| Scenario | PostgreSQL time | PostgreSQL peak RAM | SQL Server time | SQL Server peak RAM |
| --- | ---: | ---: | ---: | ---: |
| Validate workbook | 15.93 s | 9.4 MiB | 14.36 s | 9.4 MiB |
| Import, dry run | 22.15 s | 15.1 MiB | 43.03 s | 19.4 MiB |
| Import and commit | 26.24 s | 15.1 MiB | 47.09 s | 19.3 MiB |
| Strict update, dry run | 6.50 s | 15.1 MiB | 10.09 s | 19.4 MiB |
| Strict update and commit | 15.39 s | 15.1 MiB | 12.60 s | 19.4 MiB |
| Strict update rejected with one missing key | 7.22 s | 15.1 MiB | 10.45 s | 19.3 MiB |
| Mixed upsert, dry run | 23.53 s | 15.2 MiB | 38.25 s | 19.4 MiB |
| Mixed upsert and commit | 27.41 s | 15.1 MiB | 42.18 s | 19.4 MiB |
| Export, one sheet with plain header | 20.34 s | 10.1 MiB | 25.82 s | 14.4 MiB |
| Export, one sheet with merged header | 19.33 s | 10.4 MiB | 25.97 s | 14.3 MiB |
| Export, five currency sheets with plain headers | 20.69 s | 10.4 MiB | 26.34 s | 14.6 MiB |
| Export, five currency sheets with merged headers | 20.59 s | 10.3 MiB | 26.49 s | 14.6 MiB |
| Date-filtered export, plain headers (1,300 rows) | 0.16 s | 9.1 MiB | 0.19 s | 13.4 MiB |
| Date-filtered export, merged headers (1,300 rows) | 0.15 s | 9.2 MiB | 0.19 s | 13.4 MiB |
| Export, dry run | 9.51 s | 9.1 MiB | 15.64 s | 13.9 MiB |
| Export, automatic split into two sheets | 20.07 s | 10.5 MiB | 26.01 s | 14.4 MiB |

Import started with an empty fixture table. Strict update used a four-column workbook derived from the same transactions and flipped the reconciliation flag for all 500,000 rows. Mixed upsert started with 250,000 existing IDs and committed 250,000 inserts and 250,000 updates.

SQL checks confirmed the write counts, unchanged target rows in dry runs, and zero updates when strict update encountered a missing key. Workbook checks confirmed the per-currency row counts, 1,300 date-filtered rows, and two 250,000-row sheets when automatic splitting was set to that limit. Spot checks confirmed that exact decimal and long integer values survived export. Export dry runs created no workbook.

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

## Maintainer

PipeCore is maintained by [Alvan Hanani (@alvanhan)](https://github.com/alvanhan).

[MIT License](LICENSE)
