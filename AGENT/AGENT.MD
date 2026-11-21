# PipeCore 0.1.1 — Developer and agent guide

Use this guide to integrate the **Windows or Linux x86-64** package into an existing application. PipeCore is a standalone CLI for PostgreSQL and Microsoft SQL Server. It accepts an XLSX file and a JSON mapping for validation and writes, or a JSON mapping for export. The application launches it as a child process and reads the JSON response.

**Prompt for a coding agent:** “Read `AGENT/AGENT.MD` in this package. Identify my database, table, and workbook layout, then adapt the relevant example mapping and integrate the corresponding PipeCore command. Start with a read-only check or dry run.”

## 1. Choose an operation

Files are grouped by driver in [examples/portable/schemas/postgres](../examples/portable/schemas/postgres/) and [examples/portable/schemas/mssql](../examples/portable/schemas/mssql/). Copy the driver mappings and templates into your binary folder to use the commands below as written.

| File | CLI command | Purpose |
| --- | --- | --- |
| `connections.json` | `doctor --config … --connection …` | Checks a connection alias. Each alias specifies `driver` and `dsn_env`. |
| `import.json` | `import --schema … --input …` | Inserts new rows into an existing table atomically. Duplicate keys are rejected. |
| `update.json` | `mutate --schema … --input …` | Strict update. A missing key cancels the whole job. |
| `upsert.json` | `mutate --schema … --input …` | Updates matching rows and inserts new keys atomically. |
| `export.json` | `export --schema … --output …` | Exports database data to XLSX with filters, column formatting, and optional merged headers. |

For offline validation, run `validate --schema … --input …` with the same mapping as the intended import, update, or upsert. There is no separate validation mapping. Use the matching one-row workbook in [templates](../examples/portable/templates/). The three examples use a common ID so you can import it first and then update it. See the [formal JSON Schema](../schemas/pipecore.schema.json) for the complete field structure.

## 2. Connect and run

Copy this package into the application, for example `tools/pipecore/`. Run these commands from the package directory. The DSNs below target the local PipeCore Docker lab. Replace the passwords and adjust the host for the environment from which the app runs. Use verified TLS for remote instances.

```powershell
$env:PIPECORE_PG_DSN = 'host=127.0.0.1 port=15432 dbname=pipecore_test user=pipecore password=REPLACE_ME sslmode=disable'
$env:PIPECORE_MSSQL_DSN = 'Server=127.0.0.1,11433;Database=PipeCoreTest;User Id=sa;Password=REPLACE_ME;TrustServerCertificate=true'

.\pipecore.exe version
.\pipecore.exe doctor --config schemas/postgres/connections.json --connection postgres
.\pipecore.exe validate --schema schemas/postgres/import.json --input templates/import.xlsx
.\pipecore.exe import --schema schemas/postgres/import.json --input templates/import.xlsx --dry-run
.\pipecore.exe import --schema schemas/postgres/import.json --input templates/import.xlsx --report import-result.json
.\pipecore.exe mutate --schema schemas/postgres/update.json --input templates/update.xlsx --dry-run
.\pipecore.exe mutate --schema schemas/postgres/upsert.json --input templates/upsert.xlsx
.\pipecore.exe export --schema schemas/postgres/export.json --output transactions.xlsx --report export-result.json
```

On Linux, run `chmod +x ./pipecore` if needed, use `./pipecore` in place of `.\pipecore.exe`, and set environment variables with `export NAME=value`. For SQL Server, use `schemas/mssql/` and the doctor alias `mssql`. The respective drivers are `postgres` and `mssql`. The mapping's `dsn_env` names the connection environment variable. Keep credentials outside JSON and command arguments. A worker inside Docker needs the database hostname and port reachable from its own container.

- `--schema` selects the operation mapping. `--input` overrides optional `input` in the JSON. `--output` overrides optional `output`, then `workbook.filename`.
- `--report` writes a JSON summary. Create the destination's parent directory first.
- `--dry-run` checks write preconditions and staging without changing target rows. Target constraints and triggers still apply when actually writing. An export dry run does not create a workbook.

## 3. Integrate from application code

Use an argument array and an explicit child environment. Use absolute paths for the executable, mapping, upload, and output. Give each job a writable private working directory. Relative paths such as `execution.temp_dir: "./tmp"` resolve from there. The account needs table metadata and the permissions required by the operation. Writes also use database staging. An application's database transaction is separate from PipeCore's transaction.

Example in Laravel using Symfony Process. Configure `config('pipecore.pg_dsn')` from your Laravel environment and create a fresh writable `$jobDir` before running this code.

```php
use Symfony\Component\Process\Process;

$binary = base_path(PHP_OS_FAMILY === 'Windows'
    ? 'tools/pipecore/pipecore.exe'
    : 'tools/pipecore/pipecore');
$schema = base_path('tools/pipecore/schemas/postgres/export.json');
$output = $jobDir.DIRECTORY_SEPARATOR.'transactions.xlsx';
$dsn = config('pipecore.pg_dsn');
if (!is_string($dsn) || $dsn === '') {
    throw new RuntimeException('Missing PipeCore PostgreSQL DSN.');
}

$environment = ['PIPECORE_PG_DSN' => $dsn];
if (PHP_OS_FAMILY === 'Windows') {
    $environment['SystemRoot'] = getenv('SystemRoot', true)
        ?: getenv('WINDIR', true)
        ?: 'C:\\Windows';
}

$process = new Process(
    [$binary, 'export', '--schema', $schema, '--output', $output],
    $jobDir,
    $environment,
    timeout: 600,
);
$exit = $process->run();
$result = json_decode($process->getOutput(), true, 512, JSON_THROW_ON_ERROR);

if ($exit !== 0 || ($result['status'] ?? null) !== 'SUCCESS') {
    throw new RuntimeException('Export failed: '.($result['code'] ?? 'UNKNOWN'));
}
```

See [wrappers](../examples/wrappers/) for Laravel/PHP, Go, Java, and Python examples. Handle process-start errors, timeout, and malformed or missing JSON separately. For browser uploads, keep stdout and stderr separate, use an explicit timeout, and remove temporary files after the process exits. Keep finished exports and requested reports for download. PHP upload limits, disk quotas, and request/worker timeouts are separate from PipeCore. `execution.memory_limit_mb` governs workbook guard limits and is not a hard cap on process RAM.

| Exit | Meaning |
| --- | --- |
| 0 | Operation succeeded. For a real write, also verify `committed: true`. |
| 2 | Invalid arguments or mapping. |
| 3 | Workbook layout mismatch. |
| 4 | Invalid workbook data. |
| 5 | Database connection, metadata, or key precheck failed. |
| 6 | A database write failed or the commit was not confirmed. |
| 7 | File/resource failure or export failure. |
| 9 | Unexpected internal error. |

Read `code`, `summary`, `errors`, and `error_report_path` for details. Export, validation, and dry runs do not commit target writes. Reconcile the target before retrying a write that timed out or returned `commit_outcome: "unknown"`. `elapsed_ms` measures the PipeCore process and excludes the browser upload before it starts.

## 4. Write or adapt a JSON mapping

### Import, validate, and mutation

- Keep `schema_version: "1.0"`. Use `operation: "import"` for inserts or `"mutate"` for updates and upserts. Set `connection.driver`, `connection.dsn_env`, `target.schema`, and `target.table`.
- Set `excel.sheet`, `header_row`, and `first_data_row` with 1-based row numbers. The flat header must match `fields[].excel` in the listed order. Input rejects merged cells and formulas.
- For multiple sheets, replace `excel.sheet` with `"sheets": {"mode": "named", "names": ["Part 1", "Part 2"], "same_layout": true}` inside `excel`. All sheets share the same layout and transaction. Keep IDs unique across sheets.
- In each field, `excel` means the workbook label and `column` means the actual database name. Besides `type`, options include `required`, `max_length`, `trim`, `empty_as_null`, `null_tokens`, `enum`, `regex`, and `sensitive`. Supply numeric `min` and `max` as strings.
- Give decimal fields matching `precision` and `scale`. For exact decimals and long integers, use `excel_storage: "text"` and actual Excel Text cells. For text dates use a `date_format` such as `%Y-%m-%d` or `%Y-%m-%d %H:%M:%S%.f`. Offset timestamps use RFC 3339.
- Keep strict and atomic validation. `validation.max_error_samples` caps the samples in stdout. The full JSONL error report, when generated, goes to `execution.temp_dir`.

| Logical type | PostgreSQL | SQL Server |
| --- | --- | --- |
| `string` | `varchar(n)` or `text` | `nvarchar(n)` |
| `int16`, `int32`, `int64` | `smallint`, `integer`, `bigint` | `smallint`, `int`, `bigint` |
| `decimal`, `float64` | `numeric(p,s)`, `double precision` | `decimal(p,s)`, `float` |
| `boolean` | `boolean` | `bit` |
| `date`, `time` | `date`, `time(6)` | `date`, `time(6)` |
| `datetime`, `datetime_offset` | `timestamp(6)`, `timestamptz(6)` | `datetime2(6)`, `datetimeoffset(6)` |
| `uuid` | `uuid` | `uniqueidentifier` |

For **strict update**, set `mutation.mode: "update_strict"` and `mutation.match.must_match_all: true`. For **upsert**, use `mode: "upsert"` and list inserted columns in `insert_columns`. Both require a primary key or unique index covering `match.keys`. Map every key as required. Each item in `mutation.sets` has a mapped `source`, a mapped non-key `target`, and `operation: "replace"`. Keep the default serializable isolation.

### Export, filters, and grouped headers

- Set `operation: "export"`. Under `sheets[]`, choose a source table with `source.schema` and `source.table`. `columns` chooses the output order, database column, display header, width, and format. Use `format: "text"` for exact values intended as text.
- `source.filters` accepts `eq`, `gte`, `lte`, `between` (two-value array), and `in` (nonempty array). Multiple filters use AND. `source.order_by` takes a column and `asc` or `desc`. Values are parameters. The mapping does not accept raw SQL. For runtime filtering, make a per-job copy of the export JSON, allowlist column and operator choices, set values in the copy, and pass it as `--schema`.
- `header.row` places the normal column labels. Data starts on the next row. Optional `header.layout` blocks place text and colors above the normal header. Each block starts at the 1-based `row` and `column`. Inclusive `last_row` or `last_column` extends it into a merged rectangle. Supply `text` and optionally `background`, `font_color`, and `bold`. Use colors in `#RRGGBB` notation.
- Blocks must fit within the exported columns, stay above the normal header, and avoid overlap. `freeze` and `autofilter` use the normal header. With `workbook.auto_split_sheets` enabled, split sheets repeat the layout. `max_data_rows_per_sheet` is also capped by Excel's row limit minus the header height.
- To generate an input-friendly workbook, use a flat export, then match sheet name, column order, visible labels, and cell representations to the input mapping.

## 5. Example with a new table

Create the table through a migration before running PipeCore. The CLI reads existing metadata and does not create tables. Leave generated/identity columns out of input mappings, and include required insert columns or give them database defaults.

```sql
-- PostgreSQL
CREATE TABLE public.invoice_lines (
    invoice_no varchar(40) PRIMARY KEY,
    posting_date date NOT NULL,
    amount numeric(18, 2) NOT NULL
);

-- Microsoft SQL Server
CREATE TABLE dbo.invoice_lines (
    invoice_no nvarchar(40) NOT NULL PRIMARY KEY,
    posting_date date NOT NULL,
    amount decimal(18, 2) NOT NULL
);
```

Save this as `invoices-import.json` for PostgreSQL:

```json
{
  "schema_version": "1.0",
  "name": "invoice_lines_import",
  "operation": "import",
  "connection": {"driver": "postgres", "dsn_env": "PIPECORE_PG_DSN"},
  "target": {"schema": "public", "table": "invoice_lines"},
  "excel": {"sheet": "Invoices", "header_row": 1, "first_data_row": 2},
  "fields": [
    {"excel": "Invoice Number", "column": "invoice_no", "type": "string", "max_length": 40, "required": true},
    {"excel": "Posting Date", "column": "posting_date", "type": "date", "date_format": "%Y-%m-%d", "required": true},
    {"excel": "Amount", "column": "amount", "type": "decimal", "precision": 18, "scale": 2, "excel_storage": "text", "required": true}
  ]
}
```

Create `Invoices` with `Invoice Number`, `Posting Date`, and `Amount` in A1:C1. Add `INV-001`, `2026-11-21`, and `1234.50` in row 2. Format the amount as Excel Text **before** entering it. For SQL Server use `mssql`, `PIPECORE_MSSQL_DSN`, and target schema `dbo`.

To add upsert, copy the import mapping, set `operation` to `"mutate"`, and add this JSON object under `mutation`:

```json
{
  "mode": "upsert",
  "match": {"keys": ["invoice_no"], "require_unique_index": true},
  "sets": [{"source": "amount", "target": "amount", "operation": "replace"}],
  "insert_columns": ["invoice_no", "posting_date", "amount"]
}
```

For strict update, change the mode to `update_strict`, add `"must_match_all": true` inside `match`, and omit `insert_columns`.

Save this as `invoices-export.json` to export a month of invoices with a merged title:

```json
{
  "schema_version": "1.0",
  "name": "invoice_lines_export",
  "operation": "export",
  "connection": {"driver": "postgres", "dsn_env": "PIPECORE_PG_DSN"},
  "workbook": {"filename": "invoices.xlsx", "auto_split_sheets": true},
  "sheets": [
    {
      "name": "Invoices",
      "source": {
        "schema": "public",
        "table": "invoice_lines",
        "filters": [
          {"column": "posting_date", "op": "between", "value": ["2026-11-01", "2026-11-30"]}
        ],
        "order_by": [{"column": "posting_date", "direction": "asc"}]
      },
      "header": {
        "row": 2,
        "background": "#17365D",
        "font_color": "#FFFFFF",
        "bold": true,
        "freeze": true,
        "autofilter": true,
        "layout": [
          {"row": 1, "column": 1, "last_column": 3, "text": "November invoices", "background": "#245A81"}
        ]
      },
      "columns": [
        {"db_column": "invoice_no", "excel_header": "Invoice Number", "width": 22, "format": "text"},
        {"db_column": "posting_date", "excel_header": "Posting Date", "width": 18, "format": "yyyy-mm-dd"},
        {"db_column": "amount", "excel_header": "Amount", "width": 20, "format": "text", "decimal_as_text": true}
      ]
    }
  ]
}
```

Result: title merged in A1:C1, column labels on row 2, and data starting on row 3. For SQL Server, update `connection` and each `sheets[].source.schema`.

## 6. Integration checklist for agents

1. Confirm the platform, database, operation, schema/table, input sheet/header, and match key where applicable.
2. Copy the closest JSON example and adapt it to the workbook and real DDL. Prepare new tables as application migrations.
3. Wire the child process with an argument array, explicit DSN, private working directory, timeout, cleanup, and JSON result handling.
4. Run `doctor` for the connection, `validate` for input mappings, and a one-row dry run or filtered read-only export. Treat committed writes as a separate action.
