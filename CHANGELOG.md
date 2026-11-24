# Changelog

## 0.1.3 — 2026-11-24

- Added workbook-only export columns via `sheets[].columns[].value` (`null` for blank cells or fixed string, number, or boolean values) without requiring `db_column`.
- Added `workbook.split_files` to package split exports as `part-001.xlsx`, `part-002.xlsx`, ... in a `.zip` archive and report `output_format` (`xlsx` or `zip`) in the JSON output.
- Added `replace_scope` on imports to verify workbook scope and atomically delete and insert the matching subset in one transaction, reporting `summary.deleted_rows`.
- Added `mutation.delete_flag`, `mutation.scope`, and fixed-value `mutation.sets` support, and enforced `mutation.matching.require_unique_index` only when enabled.
- Mapped SQL Server `datetime` and `smalldatetime` as logical `datetime` with strict range and rounding validation on import.
- Added `validate --output` to atomically publish validated rows as NDJSON after successful validation.
- Updated `schemas/pipecore.schema.json`, `README.md`, and `.gitignore`.

## 0.1.1 — 2026-11-21

- Added optional colored cells and merged ranges above the export column headers.
- Adjusted the Excel row capacity to the configured header height and repeated
  the layout on automatically split sheets.
- Added portable examples and versioned Windows and Linux release folders.

## 0.1.0 — 2026-11-20

- Implemented the Rust CLI: version, doctor, validate, import, export, mutate.
- Added strict configuration and exact decimal/type conversion, streaming XLSX
  input, bounded error sampling, JSONL reports, private spool and disk-backed dedup.
- Added PostgreSQL COPY and SQL Server native TDS bulk staging, metadata checks,
  strict update/upsert, one transaction per job, conservative writer locks and
  bounded serialization/deadlock retries.
- Added constant-memory styled export, safe filters/sorts, exact text numerics,
  literal string output, worksheet splitting and atomic file replacement.
- Added unit/integration/CLI tests, fixture generator,
  Docker scripts, Linux/Windows packaging and CI configuration.

See IMPLEMENTATION.md for measured results and release gates still outstanding.
