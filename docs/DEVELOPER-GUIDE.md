# PipeCore developer guide

## Connect to a database

PipeCore connects to the database directly. Laravel's PDO connection and any open Laravel transaction are separate from the PipeCore process. Run PipeCore after the target table and its migration have committed.

Set only the DSN that your mapping uses. These examples assume the database has a trusted TLS certificate.

| Driver | Environment variable | DSN format |
| --- | --- | --- |
| PostgreSQL | `PIPECORE_PG_DSN` | `host=db.example.com port=5432 dbname=myapp user=app_user password=REPLACE_ME sslmode=require` |
| SQL Server | `PIPECORE_MSSQL_DSN` | `Server=db.example.com,1433;Database=MyApp;User Id=app_user;Password=REPLACE_ME;Encrypt=true;TrustServerCertificate=false` |

If Laravel runs directly on the host and you use PipeCore's development Compose stack, connect through `127.0.0.1:15432` for PostgreSQL or `127.0.0.1:11433` for SQL Server. Use the database and passwords from your local Compose configuration. A Laravel worker running inside Docker needs a hostname and port reachable from its own container. Put the binary and workbook where that worker can access them.

## First import with a small table

Create the target with a migration. This table is a minimal PostgreSQL example. On SQL Server use `dbo` and `nvarchar(40)`.

```sql
CREATE TABLE public.invoice_lines (
    invoice_no varchar(40) PRIMARY KEY,
    amount decimal(18, 2) NOT NULL
);
```

Create `invoices.xlsx` with a sheet named `Invoices`. Put `Invoice Number` in A1 and `Amount` in B1. Add one data row with `INV-001` and `1234.50`. Format the amount cell as Text before entering the value.

Save this mapping as `config/pipecore/invoices-import.json` in Laravel:

```json
{
  "schema_version": "1.0",
  "name": "invoice_lines_import",
  "operation": "import",
  "connection": {
    "driver": "postgres",
    "dsn_env": "PIPECORE_PG_DSN"
  },
  "target": { "schema": "public", "table": "invoice_lines" },
  "excel": {
    "sheet": "Invoices",
    "header_row": 1,
    "first_data_row": 2,
    "header_policy": "exact_order"
  },
  "fields": [
    {
      "excel": "Invoice Number",
      "column": "invoice_no",
      "type": "string",
      "required": true,
      "max_length": 40,
      "trim": true
    },
    {
      "excel": "Amount",
      "column": "amount",
      "type": "decimal",
      "required": true,
      "precision": 18,
      "scale": 2,
      "excel_storage": "text"
    }
  ]
}
```

## Adapt a JSON mapping

| Change | Fields to edit |
| --- | --- |
| Connect to another database | Set `connection.driver` and `connection.dsn_env`. Set the schema and table in `target` or each `sheets[].source`. |
| Change the workbook layout | Set `excel.sheet`, `header_row`, and `first_data_row` using 1-based row numbers. Use `excel.sheets` for named input sheets with the same layout. |
| Map input columns | Add one entry to `fields` per Excel column in header order. `excel` is the header text. `column` is the database column. Match `type`, length, precision, and scale to the database. |
| Apply data rules | Use `required`, `trim`, `empty_as_null`, `null_tokens`, `enum`, `regex`, or numeric `min` and `max` expressed as JSON strings. Use `sensitive` for fields whose values should be masked in diagnostics. |
| Update or upsert | Set `operation` to `mutate`, then choose `mutation.mode`. `match.keys` names the database columns covered by a unique index. `sets` maps input columns to destination columns with `operation: replace`. For `upsert`, list insert columns, including keys and required values, in `insert_columns`. |
| Customize export | Set `operation` to `export`. In each `sheets` entry, define `source`, `columns`, and `header`. Add `source.filters` and `source.order_by` as needed. Excel column order follows `columns`. |

Strict updates require every key to be found and roll back as one job if any key is missing. Import rejects existing keys. Use upsert intentionally when you want both updates and inserts. Omit identity and generated target columns. For exports, the supported filter operators are `eq`, `gte`, `lte`, `between`, and `in`.

For SQL Server text timestamps exported with a `T` separator, set the matching input field's `date_format` to `%Y-%m-%dT%H:%M:%S%.f`. Workbook headers may also need their own import mapping. See the [field reference](../schemas/pipecore.schema.json) and the complete [import](../examples/import-finance.json), [update](../examples/bulk-update-finance.json), [upsert](../examples/upsert-finance.json), and [export](../examples/export-finance.json) examples.

## Run it from Laravel

1. Copy the platform binary into your Laravel project's `bin/` folder. On Linux, run `chmod +x bin/pipecore`.
2. Add the mapping above in `config/pipecore/`. Place `invoices.xlsx` in `storage/app/private/`.
3. Put the DSN in Laravel's `.env`. Use quoted DSN values and keep credentials out of version control.
4. Copy the [Artisan command example](../examples/wrappers/laravel.php) into `routes/console.php`, below its existing opening `<?php` tag. Add the configuration file below.

`config/pipecore.php`:

```php
<?php

return [
    'pg_dsn' => env('PIPECORE_PG_DSN'),
    'mssql_dsn' => env('PIPECORE_MSSQL_DSN'),
];
```

Laravel reads environment variables through the configuration file. The command passes the configured DSN to the child process explicitly, which also works when Laravel configuration is cached. It uses Symfony Process with an argument array. If the class is missing from your project, run `composer require symfony/process`. [Laravel configuration](https://laravel.com/docs/12.x/configuration) · [Symfony Process](https://symfony.com/doc/current/components/process.html)

Run these from the Laravel project root:

```sh
php artisan config:clear
php artisan pipecore:import-invoices --validate
php artisan pipecore:import-invoices
php artisan pipecore:import-invoices --commit
```

The first command mode validates offline. The default import mode checks the database and stages the input with `--dry-run`. The explicit `--commit` mode writes rows. For SQL Server, change the mapping to the SQL Server connection and target schema, then add `--mssql` to the import command. The command's database option and the mapping must agree.

## Handle results in your application

Use the process exit code for control flow and the JSON `code` to categorize failures. `errors` contains bounded samples with fields such as `sheet`, `row`, and `column`. When `error_report_path` is present, the JSONL file contains detailed diagnostics. Keep uploads, reports, and temporary work files private.

For user uploads, pass application-managed absolute paths and choose an application-owned mapping. For jobs that may outlast a request, run the process from a queue worker with an explicit timeout and a writable working directory. Avoid blind retries after a timeout or an unknown commit outcome. Confirm the affected database records first.
