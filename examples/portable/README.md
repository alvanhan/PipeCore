# PipeCore 0.1.1

Copy this folder for your operating system into your project. PipeCore connects directly to PostgreSQL or Microsoft SQL Server and returns one JSON response on stdout. Use `pipecore.exe` on Windows and `./pipecore` on Linux. On Linux, run `chmod +x ./pipecore` if needed. The Linux build targets Debian 12 compatible systems with glibc 2.36 and OpenSSL 3.

## Choose an operation

| Operation | What it does | JSON / workbook |
| --- | --- | --- |
| Check connection | Checks the selected database | `connections.json` |
| Validate XLSX | Checks headers and values offline | `import.json`, `update.json`, or `upsert.json` with its matching XLSX |
| Import | Inserts new rows atomically | `import.json` + `templates/import.xlsx` |
| Strict update | Updates existing IDs. A missing ID cancels the job. | `update.json` + `templates/update.xlsx` |
| Upsert | Updates existing IDs or inserts new rows | `upsert.json` + `templates/upsert.xlsx` |
| Export | Exports 25 columns with a grouped header | `export.json` |

The JSON files are in `schemas/postgres/` and `schemas/mssql/`. Each input workbook has one sample record. All three samples share an ID so you can import first and then update it. Change the ID to try the insert path of upsert.

## Connect and run

Set only the DSN you use. Examples below show PowerShell for the local Docker setup. Replace the password and edit the host/database for a different environment. Use verified TLS for a remote environment.

```powershell
$env:PIPECORE_PG_DSN = 'host=127.0.0.1 port=15432 dbname=pipecore_test user=pipecore password=REPLACE_ME sslmode=disable'
$env:PIPECORE_MSSQL_DSN = 'Server=127.0.0.1,11433;Database=PipeCoreTest;User Id=sa;Password=REPLACE_ME;TrustServerCertificate=true'

.\pipecore.exe version
.\pipecore.exe doctor --config schemas/postgres/connections.json --connection postgres
.\pipecore.exe validate --schema schemas/postgres/import.json --input templates/import.xlsx
.\pipecore.exe import --schema schemas/postgres/import.json --input templates/import.xlsx --dry-run
.\pipecore.exe import --schema schemas/postgres/import.json --input templates/import.xlsx
.\pipecore.exe mutate --schema schemas/postgres/update.json --input templates/update.xlsx
.\pipecore.exe mutate --schema schemas/postgres/upsert.json --input templates/upsert.xlsx
.\pipecore.exe export --schema schemas/postgres/export.json --output transactions.xlsx
```

For SQL Server, use `schemas/mssql/` and `--connection mssql` with doctor. On Linux, set the DSN with `export NAME='value'` and substitute `./pipecore` for `.\pipecore.exe`. From application code, pass an argument array, set the DSN in the child process environment, and parse stdout as JSON. Check the exit code and `committed` for writes. Keep exact decimals and long integers as text in Excel.

## Customize the export

- `sheets[].header.row` is the 1-based row of the normal column headers. Set it to `2` for data from row 3, or `5` for data from row 6. PipeCore adjusts the maximum data rows per sheet to fit the header.
- `sheets[].header.layout` defines optional cells and merged ranges above that row. `row` and `column` are 1-based. Add `last_row` or `last_column` to merge. Omit them for a single cell. Each block can set `background`, `font_color`, and `bold`.
- `sheets[].columns` controls output column names and order. Layout column numbers follow that order. Merge ranges must stay within the exported columns, above the normal header, and cannot overlap. Unspecified cells stay blank.
- `sheets[].source.filters` limits the export by column and value using `eq`, `gte`, `lte`, `between`, or `in`. See the source table and columns in the provided JSON before adjusting the mapping.

The bundled export example puts a title in row 1, colored groups in row 2, column labels in row 3, and data from row 4. Every sheet created by auto-split repeats the layout. Use the separate input templates for imports, which still require a flat, unmerged header.
