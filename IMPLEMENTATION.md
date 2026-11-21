# Implementation and verification

Date: 20 November 2026. This document describes the v0.1.0 implementation and its verification.
This file distinguishes implemented core behavior from verified release readiness.

## Implemented

- M0: one Rust crate, strict serde configuration and semantic checks, JSON CLI,
  doctor, Docker setup, generated JSON Schema, CI configuration.
- M1: calamine cell streaming with formula detection, header checks, exact decimal
  strings up to precision 38, typed validation, row provenance, JSONL errors,
  disk-backed duplicate detection and private spool.
- M2/M3: metadata introspection, PostgreSQL COPY, SQL Server native bulk transport,
  typed staging and atomic target insert with rollback on failure.
- M4: native streaming SELECT, parameterized filters, constant-memory styled XLSX,
  multi-sheet/split handling, exact numerics as text and atomic output replacement.
- M5/M6: unique-key metadata checks, source duplicate detection under database
  comparison rules, missing-key reports, strict update, atomic update-plus-insert,
  protected concurrent writes and bounded serialization/deadlock retries.
- M7: release and audit tooling are available; execution results are recorded below.

## Verification ledger

- The final source passed Clippy and 22 tests, including Docker integration against PostgreSQL and SQL Server.
- The preceding Windows release build completed native 4,000-row import, update, upsert, and export smoke checks on both databases. Microsoft Excel opened both exported workbooks and preserved the exact Debit value and header styling.
- Local dependency audit found zero vulnerabilities and one maintenance advisory for chrono.
- The 100,000-row benchmark completed database writes and export on both drivers. PostgreSQL passed validation of all 125,000 exported rows. SQL Server export uses an ISO T separator for text datetime. Its round-trip benchmark stopped because the import example expects a space separator. See docs/DEVELOPER-GUIDE.md for field configuration.
- The one-million-row benchmark and broad runtime compatibility checks remain open.

## Explicit differences and limits

- No production-readiness claim for all 30 original acceptance gates. See the
  completed-run ledger before distributing this version.
- Dry-run performs metadata, validation, staging and key checks without target
  DML. It cannot prove target CHECK/FK/trigger behavior; real jobs remain atomic
  when those constraints reject the write.
- Initial writer concurrency uses conservative table locks. It prioritizes
  correctness; concurrency throughput needs workload-specific tuning.
- The native SQL Server bulk wire table uses Unicode text to retain all 38 decimal
  digits, then converts into a staging table with inspected database types. The
  completed benchmark included that conversion overhead.
- Memory settings bound eager metadata and row buffers, not total OS RSS. Workbook
  XML, record count, shared strings and worksheets have explicit guards. Hard
  memory/disk/time limits remain deployment-level responsibilities.
- Forced-kill spool recovery is manual. Normal completion/failure and cooperative
  cancellation drop temporary files; error reports remain for the caller.
- Export decimal and int64 values always use exact text. datetime_offset output is
  text; PostgreSQL normalizes it to UTC. SQL Server varchar/char accepts only ASCII
  input to prevent codepage substitution; use nvarchar for Unicode.
- No arbitrary SQL, cross-database transactions, non-atomic batch commits,
  expression updates, flexible headers, merged cells or macros.
- GitHub Actions runs the CI workflow for pushes and pull requests.
