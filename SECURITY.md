# Security

Do not include credentials, raw database errors, financial input files, or real
customer data in public issues. Report a vulnerability privately to the repository
owner through the repository's GitHub security reporting options.

DSNs are read from environment variables. PostgreSQL uses native TLS and honors
its DSN TLS policy; production should use sslmode=require with a trusted server certificate. SQL Server should
use Encrypt=true and TrustServerCertificate=false. The Compose DSNs are local
development settings only. Use least-privilege database roles in production.

Temporary spool files are private files; use a private temp_dir on Windows with
an appropriate ACL. JSONL error reports are retained intentionally; configure
external retention and mark sensitive fields. Files left after a forced process
kill should be removed by the operator only after confirming no job uses them.

Workbook guards bound ZIP entry count, decompressed size, sheet count, and eager
metadata size. These are defense in depth, not an OS sandbox. For hostile uploads,
run the CLI under OS/container memory, disk, time and concurrency limits.
