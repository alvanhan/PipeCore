param([switch]$Integration)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
$checks = @(@('fmt','--all','--','--check'),@('clippy','--all-targets','--','-D','warnings'),@('test','--all-targets'))
foreach ($check in $checks) { docker compose run --rm rust-dev cargo @check; if ($LASTEXITCODE) { throw "Cargo check failed: $($check -join ' ')" } }
if ($Integration) { docker compose run --rm rust-dev cargo test --test integration -- --ignored --test-threads=1; if ($LASTEXITCODE) { throw 'Integration tests failed' } }
