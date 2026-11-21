# Contributing

Read README.md and docs/DEVELOPER-GUIDE.md for the CLI and mapping contract. Use one Rust crate and keep PostgreSQL and
SQL Server behavior equivalent. Do not round financial values or commit partial
strict jobs. A new behavior must include a targeted regression test.

Run `./scripts/dev.ps1` then `./scripts/test.ps1 -Integration`. Integration tests
create uniquely named schemas in the development databases and remove them when
they pass. A failed test may leave its isolated `pc_*` schema for inspection.
Never point integration tests at a production database.

`cargo run --example schema` generates `schemas/pipecore.schema.json`. Semantic
checks in config.rs supplement the structural JSON Schema. Keep both up to date.

Run `./scripts/release.ps1` to cross-build and package. Verify the actual Windows
binary and a Linux runtime without the Rust toolchain before distributing.
