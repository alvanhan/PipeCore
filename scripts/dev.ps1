param([switch]$SkipBuild)
$ErrorActionPreference = 'Stop'
Set-Location (Split-Path $PSScriptRoot -Parent)
if (!(Test-Path .env)) { throw 'Create .env from .env.example and configure development passwords first.' }
if (!$SkipBuild) { docker compose build rust-dev; if ($LASTEXITCODE) { throw 'Docker build failed' } }
docker compose up -d postgres mssql
if ($LASTEXITCODE) { throw 'Database startup failed' }
docker compose cp tests/fixtures/sql/postgres.sql postgres:/tmp/pipecore-fixture.sql
docker compose cp tests/fixtures/sql/mssql.sql mssql:/tmp/pipecore-fixture.sql
$ready = $false
for ($attempt = 0; $attempt -lt 60; $attempt++) {
    docker compose exec -T mssql bash -c '/opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P "$MSSQL_SA_PASSWORD" -C -b -Q "SELECT 1"' *> $null
    if ($LASTEXITCODE -eq 0) { $ready = $true; break }
    Start-Sleep -Seconds 2
}
if (!$ready) { throw 'SQL Server did not become ready' }
docker compose exec -T postgres sh -c 'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -v ON_ERROR_STOP=1 -f /tmp/pipecore-fixture.sql'
if ($LASTEXITCODE) { throw 'PostgreSQL fixture failed' }
docker compose exec -T mssql bash -c '/opt/mssql-tools18/bin/sqlcmd -S localhost -U sa -P "$MSSQL_SA_PASSWORD" -C -b -i /tmp/pipecore-fixture.sql'
if ($LASTEXITCODE) { throw 'SQL Server fixture failed' }
