# Changelog

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
