use crate::{
    config::{Config, Connection, Table},
    excel::Spool,
    report::{Detail, Report, column},
};
use anyhow::{Result, ensure};
use futures_util::{SinkExt, TryStreamExt};
use std::collections::HashMap;
use tokio::net::TcpStream;
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt};

type MsClient = tiberius::Client<Compat<TcpStream>>;
pub enum Db {
    Pg(tokio_postgres::Client),
    Ms(Box<MsClient>),
}
#[derive(Debug, Clone, PartialEq)]
pub struct Meta {
    pub name: String,
    pub kind: String,
    pub length: Option<usize>,
    pub precision: Option<u32>,
    pub scale: Option<u32>,
    pub nullable: bool,
    pub generated: bool,
    pub defaulted: bool,
    pub time_precision: Option<u32>,
}
impl Meta {
    pub fn sql_type(&self, pg: bool) -> Result<String> {
        Ok(match self.kind.as_str() {
            "character varying" | "varchar" | "nvarchar" | "char" | "nchar" | "character" => {
                if pg {
                    self.length
                        .map_or("text".into(), |n| format!("varchar({n})"))
                } else {
                    self.length
                        .map_or("nvarchar(max)".into(), |n| format!("nvarchar({n})"))
                }
            }
            "text" => {
                if pg {
                    "text".into()
                } else {
                    "nvarchar(max)".into()
                }
            }
            "numeric" | "decimal" => format!(
                "decimal({},{})",
                self.precision.unwrap_or(38),
                self.scale.unwrap_or(0)
            ),
            "smallint" => "smallint".into(),
            "integer" | "int" => {
                if pg {
                    "integer".into()
                } else {
                    "int".into()
                }
            }
            "bigint" => "bigint".into(),
            "double precision" | "float" => {
                if pg {
                    "double precision".into()
                } else {
                    "float(53)".into()
                }
            }
            "boolean" | "bit" => {
                if pg {
                    "boolean".into()
                } else {
                    "bit".into()
                }
            }
            "date" => "date".into(),
            "time without time zone" | "time" => {
                format!("time({})", self.time_precision.unwrap_or(6))
            }
            "timestamp without time zone" | "datetime2" => format!(
                "{}({})",
                if pg { "timestamp" } else { "datetime2" },
                self.time_precision.unwrap_or(6)
            ),
            "timestamp with time zone" | "datetimeoffset" => format!(
                "{}({})",
                if pg { "timestamptz" } else { "datetimeoffset" },
                self.time_precision.unwrap_or(6)
            ),
            "datetime" | "smalldatetime" if !pg => self.kind.clone(),
            "uuid" | "uniqueidentifier" => {
                if pg {
                    "uuid".into()
                } else {
                    "uniqueidentifier".into()
                }
            }
            _ => anyhow::bail!("Unsupported database column type"),
        })
    }
    pub fn logical(&self) -> &str {
        match self.kind.as_str() {
            "character varying" | "varchar" | "nvarchar" | "char" | "nchar" | "character"
            | "text" => "string",
            "numeric" | "decimal" => "decimal",
            "smallint" => "int16",
            "integer" | "int" => "int32",
            "bigint" => "int64",
            "double precision" | "float" => "float64",
            "boolean" | "bit" => "boolean",
            "date" => "date",
            "time without time zone" | "time" => "time",
            "timestamp without time zone" | "datetime2" | "datetime" | "smalldatetime" => {
                "datetime"
            }
            "timestamp with time zone" | "datetimeoffset" => "datetime_offset",
            "uuid" | "uniqueidentifier" => "uuid",
            _ => "unsupported",
        }
    }
}
fn field_sql_type(f: &crate::config::Field, meta: &[Meta], pg: bool) -> Result<String> {
    if let Some(m) = meta.iter().find(|m| m.name == f.column) {
        return m.sql_type(pg);
    }
    Ok(match f.kind.as_str() {
        "string" => if pg { "text" } else { "nvarchar(max)" }.into(),
        "int16" => "smallint".into(),
        "int32" => if pg { "integer" } else { "int" }.into(),
        "int64" => "bigint".into(),
        "decimal" => format!(
            "decimal({},{})",
            f.precision.unwrap_or(38),
            f.scale.unwrap_or(0)
        ),
        "float64" => if pg { "double precision" } else { "float(53)" }.into(),
        "boolean" => if pg { "boolean" } else { "bit" }.into(),
        "date" => "date".into(),
        "time" => "time(6)".into(),
        "datetime" => if pg { "timestamp(6)" } else { "datetime2(6)" }.into(),
        "datetime_offset" => if pg { "timestamptz(6)" } else { "datetimeoffset(6)" }.into(),
        "uuid" => if pg { "uuid" } else { "uniqueidentifier" }.into(),
        _ => anyhow::bail!("Unsupported virtual column type"),
    })
}

impl Db {
    pub async fn connect(c: &Connection) -> Result<Self> {
        let dsn = std::env::var(&c.dsn_env)
            .map_err(|_| anyhow::anyhow!("Connection environment variable missing"))?;
        if c.driver == "postgres" {
            let connector = postgres_native_tls::MakeTlsConnector::new(
                native_tls::TlsConnector::builder().build()?,
            );
            let (client, connection) =
                tokio_postgres::connect(&dsn, connector)
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!("PostgreSQL connection failed (check DSN, network and TLS)")
                    })?;
            tokio::spawn(async move {
                let _ = connection.await;
            });
            Ok(Self::Pg(client))
        } else {
            let config = tiberius::Config::from_ado_string(&dsn)
                .map_err(|_| anyhow::anyhow!("Invalid SQL Server DSN"))?;
            let tcp = TcpStream::connect(config.get_addr())
                .await
                .map_err(|_| anyhow::anyhow!("SQL Server network connection failed"))?;
            tcp.set_nodelay(true)?;
            let client = tiberius::Client::connect(config, tcp.compat_write())
                .await
                .map_err(|_| {
                    anyhow::anyhow!(
                        "SQL Server connection failed (check credentials, database and TLS)"
                    )
                })?;
            Ok(Self::Ms(Box::new(client)))
        }
    }
    pub fn pg(&self) -> bool {
        matches!(self, Self::Pg(_))
    }
    pub fn q(&self, n: &str) -> String {
        if self.pg() {
            format!("\"{}\"", n.replace('"', "\"\""))
        } else {
            format!("[{}]", n.replace(']', "]]"))
        }
    }
    pub fn table(&self, t: &Table) -> String {
        format!("{}.{}", self.q(&t.schema), self.q(&t.table))
    }
    pub fn text(&self, s: &str) -> String {
        if self.pg() {
            format!("CAST({s} AS text)")
        } else {
            format!("CONVERT(nvarchar(max),{s},127)")
        }
    }
    pub fn param(&self, i: usize) -> String {
        if self.pg() {
            format!("${i}")
        } else {
            format!("@P{i}")
        }
    }
    pub async fn batch(&mut self, sql: &str) -> Result<()> {
        match self {
            Self::Pg(c) => c.batch_execute(sql).await?,
            Self::Ms(c) => {
                c.simple_query(sql).await?.into_results().await?;
            }
        }
        Ok(())
    }
    pub async fn execute(&mut self, sql: &str) -> Result<u64> {
        self.execute_bound(sql, &[]).await
    }
    async fn execute_bound(&mut self, sql: &str, params: &[String]) -> Result<u64> {
        Ok(match self {
            Self::Pg(c) => {
                let values: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                    params.iter().map(|value| value as _).collect();
                c.execute(sql, &values).await?
            }
            Self::Ms(c) => {
                // DONE tokens also include trigger writes. Report the outer DML count.
                let values: Vec<&dyn tiberius::ToSql> =
                    params.iter().map(|value| value as _).collect();
                let mut rows = c
                    .query(
                        format!("{sql}; SELECT CONVERT(nvarchar(max),@@ROWCOUNT) AS __pc_count"),
                        &values,
                    )
                    .await?
                    .into_row_stream();
                let mut count = None;
                while let Some(row) = rows.try_next().await? {
                    if row.len() == 1 && row.columns()[0].name() == "__pc_count" {
                        count = Some(
                            row.try_get::<&str, _>(0)?
                                .ok_or_else(|| anyhow::anyhow!("Missing affected count"))?
                                .parse::<u64>()?,
                        );
                    }
                }
                count.ok_or_else(|| anyhow::anyhow!("Missing affected count"))?
            }
        })
    }
    pub async fn stream(
        &mut self,
        sql: &str,
        params: &[String],
        mut f: impl FnMut(Vec<Option<String>>) -> Result<()>,
    ) -> Result<()> {
        match self {
            Self::Pg(c) => {
                let ps: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> =
                    params.iter().map(|s| s as _).collect();
                let rows = c.query_raw(sql, ps).await?;
                futures_util::pin_mut!(rows);
                while let Some(row) = rows.try_next().await? {
                    let mut v = Vec::with_capacity(row.len());
                    for i in 0..row.len() {
                        v.push(row.try_get::<_, Option<String>>(i)?);
                    }
                    f(v)?;
                }
            }
            Self::Ms(c) => {
                let ps: Vec<&dyn tiberius::ToSql> = params.iter().map(|s| s as _).collect();
                let mut rows = c.query(sql, &ps).await?.into_row_stream();
                while let Some(row) = rows.try_next().await? {
                    let mut v = Vec::with_capacity(row.len());
                    for i in 0..row.len() {
                        v.push(row.try_get::<&str, _>(i)?.map(String::from));
                    }
                    f(v)?;
                }
            }
        }
        Ok(())
    }
    pub async fn query(
        &mut self,
        sql: &str,
        params: &[String],
    ) -> Result<Vec<Vec<Option<String>>>> {
        let mut rows = vec![];
        self.stream(sql, params, |r| {
            ensure!(rows.len() < 100000, "Metadata result limit exceeded");
            rows.push(r);
            Ok(())
        })
        .await?;
        Ok(rows)
    }
    pub async fn scalar(&mut self, sql: &str) -> Result<u64> {
        self.scalar_bound(sql, &[]).await
    }
    async fn scalar_bound(&mut self, sql: &str, params: &[String]) -> Result<u64> {
        let r = self.query(sql, params).await?;
        Ok(r[0][0].as_ref().unwrap().parse()?)
    }
    pub async fn inspect(&mut self, t: &Table) -> Result<Vec<Meta>> {
        let sql = if self.pg() {
            "SELECT column_name::text,data_type::text,character_maximum_length::text,numeric_precision::text,numeric_scale::text,is_nullable::text,(CASE WHEN is_identity='YES' OR is_generated<>'NEVER' THEN '1' ELSE '0' END)::text,(CASE WHEN column_default IS NULL THEN '0' ELSE '1' END)::text,datetime_precision::text FROM information_schema.columns WHERE table_schema=$1 AND table_name=$2 ORDER BY ordinal_position".to_string()
        } else {
            "SELECT CONVERT(nvarchar(max),COLUMN_NAME),CONVERT(nvarchar(max),DATA_TYPE),CONVERT(nvarchar(max),CHARACTER_MAXIMUM_LENGTH),CONVERT(nvarchar(max),NUMERIC_PRECISION),CONVERT(nvarchar(max),NUMERIC_SCALE),CONVERT(nvarchar(max),IS_NULLABLE),CONVERT(nvarchar(max),CASE WHEN COLUMNPROPERTY(OBJECT_ID(QUOTENAME(TABLE_SCHEMA)+'.'+QUOTENAME(TABLE_NAME)),COLUMN_NAME,'IsIdentity')=1 OR COLUMNPROPERTY(OBJECT_ID(QUOTENAME(TABLE_SCHEMA)+'.'+QUOTENAME(TABLE_NAME)),COLUMN_NAME,'IsComputed')=1 THEN 1 ELSE 0 END),CONVERT(nvarchar(max),CASE WHEN COLUMN_DEFAULT IS NULL THEN 0 ELSE 1 END),CONVERT(nvarchar(max),DATETIME_PRECISION) FROM INFORMATION_SCHEMA.COLUMNS WHERE TABLE_SCHEMA=@P1 AND TABLE_NAME=@P2 ORDER BY ORDINAL_POSITION".to_string()
        };
        let rows = self
            .query(&sql, &[t.schema.clone(), t.table.clone()])
            .await?;
        ensure!(!rows.is_empty(), "TARGET_SCHEMA_MISMATCH");
        rows.into_iter()
            .map(|r| {
                Ok(Meta {
                    name: r[0].clone().unwrap(),
                    kind: r[1].clone().unwrap(),
                    length: r[2].as_ref().and_then(|s| s.parse().ok()),
                    precision: r[3].as_ref().and_then(|s| s.parse().ok()),
                    scale: r[4].as_ref().and_then(|s| s.parse().ok()),
                    nullable: r[5].as_deref() == Some("YES"),
                    generated: r[6].as_deref() == Some("1"),
                    defaulted: r[7].as_deref() == Some("1"),
                    time_precision: r[8].as_ref().and_then(|s| s.parse().ok()),
                })
            })
            .collect()
    }
    pub async fn unique_keys(&mut self, t: &Table) -> Result<Vec<Vec<String>>> {
        let sql = if self.pg() {
            "SELECT i.indexrelid::text,a.attname::text FROM pg_index i JOIN pg_class t ON t.oid=i.indrelid JOIN pg_namespace n ON n.oid=t.relnamespace JOIN LATERAL unnest(i.indkey) WITH ORDINALITY k(attnum,ord) ON k.ord<=i.indnkeyatts JOIN pg_attribute a ON a.attrelid=t.oid AND a.attnum=k.attnum WHERE n.nspname=$1 AND t.relname=$2 AND i.indisunique AND i.indisvalid AND i.indpred IS NULL AND i.indexprs IS NULL ORDER BY i.indisprimary DESC,i.indexrelid,k.ord"
        } else {
            "SELECT CONVERT(nvarchar(max),i.index_id),CONVERT(nvarchar(max),c.name) FROM sys.indexes i JOIN sys.tables t ON t.object_id=i.object_id JOIN sys.schemas s ON s.schema_id=t.schema_id JOIN sys.index_columns ic ON ic.object_id=i.object_id AND ic.index_id=i.index_id JOIN sys.columns c ON c.object_id=ic.object_id AND c.column_id=ic.column_id WHERE s.name=@P1 AND t.name=@P2 AND i.is_unique=1 AND i.is_disabled=0 AND i.has_filter=0 AND ic.key_ordinal>0 ORDER BY i.is_primary_key DESC,i.index_id,ic.key_ordinal"
        };
        let rows = self
            .query(sql, &[t.schema.clone(), t.table.clone()])
            .await?;
        let mut keys: Vec<Vec<String>> = vec![];
        let mut last = None;
        for r in rows {
            if last != r[0] {
                keys.push(vec![]);
                last = r[0].clone();
            }
            keys.last_mut().unwrap().push(r[1].clone().unwrap());
        }
        Ok(keys)
    }
    pub async fn begin(&mut self) -> Result<()> {
        self.batch(if self.pg(){"BEGIN ISOLATION LEVEL SERIALIZABLE; SET LOCAL statement_timeout='300s'; SET LOCAL lock_timeout='30s'"}else{"SET XACT_ABORT ON; SET LOCK_TIMEOUT 30000; SET TRANSACTION ISOLATION LEVEL SERIALIZABLE; BEGIN TRANSACTION"}).await
    }
    pub async fn rollback(&mut self) -> Result<()> {
        self.batch(if self.pg() {
            "ROLLBACK"
        } else {
            "IF @@TRANCOUNT > 0 ROLLBACK TRANSACTION"
        })
        .await
    }
    pub async fn commit(&mut self) -> Result<()> {
        self.batch("COMMIT").await
    }
    pub async fn stage(&mut self, c: &Config, meta: &[Meta], spool: &Spool) -> Result<String> {
        let pg = self.pg();
        let stage = if pg { "pc_stage" } else { "#pc_stage" };
        let defs: Vec<_> = c
            .fields
            .iter()
            .map(|f| Ok(format!("{} {}", self.q(&f.column), field_sql_type(f, meta, pg)?)))
            .collect::<Result<_>>()?;
        let provenance = if pg {
            "__excel_sheet text NOT NULL, __excel_row bigint NOT NULL"
        } else {
            "__excel_sheet nvarchar(128) NOT NULL, __excel_row bigint NOT NULL"
        };
        self.batch(&format!(
            "CREATE {}TABLE {stage} ({}, {provenance})",
            if pg { "TEMP " } else { "" },
            defs.join(",")
        ))
        .await?;
        let columns = c
            .fields
            .iter()
            .map(|f| self.q(&f.column))
            .chain(["__excel_sheet".into(), "__excel_row".into()])
            .collect::<Vec<_>>();
        let select = if pg {
            String::new()
        } else {
            let mut values = Vec::with_capacity(columns.len());
            for (field, column) in c.fields.iter().zip(&columns) {
                if field.kind == "datetime" {
                    values.push(format!(
                        "CONVERT({}, {column}, 121)",
                        field_sql_type(field, meta, pg)?
                    ));
                } else {
                    values.push(column.clone());
                }
            }
            values.extend(columns[c.fields.len()..].iter().cloned());
            values.join(",")
        };
        match self {
            Self::Pg(client) => {
                let sink = client
                    .copy_in(&format!("COPY {stage} ({}) FROM STDIN", columns.join(",")))
                    .await?;
                futures_util::pin_mut!(sink);
                let mut buf = String::with_capacity(65536);
                for row in spool.rows()? {
                    crate::check_cancelled()?;
                    let row = row?;
                    for v in row
                        .values
                        .iter()
                        .chain([&Some(row.sheet), &Some(row.row.to_string())])
                    {
                        if let Some(v) = v {
                            buf.push_str(
                                &v.replace('\\', "\\\\")
                                    .replace('\t', "\\t")
                                    .replace('\n', "\\n")
                                    .replace('\r', "\\r"),
                            );
                        } else {
                            buf.push_str("\\N");
                        }
                        buf.push('\t');
                    }
                    buf.pop();
                    buf.push('\n');
                    if buf.len() >= 65536 {
                        sink.send(bytes::Bytes::from(std::mem::take(&mut buf)))
                            .await?;
                    }
                }
                if !buf.is_empty() {
                    sink.send(bytes::Bytes::from(buf)).await?;
                }
                ensure!(
                    sink.finish().await? == spool.count,
                    "Staging count mismatch"
                );
            }
            Self::Ms(client) => {
                // Text transport preserves all 38 decimal digits; server converts to validated target types.
                let defs = columns
                    .iter()
                    .map(|n| format!("{n} nvarchar(max) NULL"))
                    .collect::<Vec<_>>()
                    .join(",");
                client
                    .simple_query(format!("CREATE TABLE #pc_wire ({defs})"))
                    .await?
                    .into_results()
                    .await?;
                let names: Vec<&str> = c
                    .fields
                    .iter()
                    .map(|f| f.column.as_str())
                    .chain(["__excel_sheet", "__excel_row"])
                    .collect();
                let mut bulk = client.bulk_insert_columns("#pc_wire", &names).await?;
                for row in spool.rows()? {
                    crate::check_cancelled()?;
                    let row = row?;
                    let mut token = tiberius::TokenRow::new();
                    for v in row
                        .values
                        .into_iter()
                        .chain([Some(row.sheet), Some(row.row.to_string())])
                    {
                        token.push(tiberius::ColumnData::String(v.map(std::borrow::Cow::Owned)));
                    }
                    bulk.send(token).await?;
                }
                bulk.finalize().await?;
                client
                    .execute(
                        format!(
                            "INSERT INTO {stage} ({0}) SELECT {select} FROM #pc_wire",
                            columns.join(",")
                        ),
                        &[],
                    )
                    .await?;
                client
                    .simple_query("DROP TABLE #pc_wire")
                    .await?
                    .into_results()
                    .await?;
            }
        }
        ensure!(
            self.scalar(&format!("SELECT {} FROM {stage}", self.text("COUNT(*)")))
                .await?
                == spool.count,
            "Staging count mismatch"
        );
        Ok(stage.into())
    }
}
pub fn precheck(c: &Config, meta: &[Meta]) -> Result<()> {
    for f in &c.fields {
        let Some(m) = meta.iter().find(|m| m.name == f.column) else {
            ensure!(c.is_delete_flag(&f.column), "TARGET_SCHEMA_MISMATCH");
            continue;
        };
        ensure!(
            !m.generated && m.logical() == f.kind,
            "TARGET_SCHEMA_MISMATCH"
        );
        if f.kind == "decimal" {
            ensure!(
                m.scale.is_some_and(|s| s >= f.scale.unwrap())
                    && m.precision
                        .zip(m.scale)
                        .is_some_and(|(p, s)| p - s >= f.precision.unwrap() - f.scale.unwrap()),
                "TARGET_SCHEMA_MISMATCH"
            );
        }
        if let Some(n) = m.length {
            ensure!(
                f.max_length.is_some_and(|l| l <= n),
                "TARGET_SCHEMA_MISMATCH"
            );
        }
    }
    let inserts: Vec<&str> = if c.operation == "import" {
        c.fields.iter().map(|f| f.column.as_str()).collect()
    } else if c.mutation.as_ref().is_some_and(|m| m.mode == "upsert") {
        c.mutation
            .as_ref()
            .unwrap()
            .insert_columns
            .iter()
            .map(String::as_str)
            .collect()
    } else {
        vec![]
    };
    if c.operation == "import" || c.mutation.as_ref().is_some_and(|m| m.mode == "upsert") {
        for m in meta {
            ensure!(
                m.nullable || m.generated || m.defaulted || inserts.contains(&m.name.as_str()),
                "TARGET_REQUIRED_COLUMN_MISSING"
            );
        }
    }
    if let Some(m) = &c.mutation {
        for filter in &m.scope {
            ensure!(
                meta.iter().any(|col| col.name == filter.column),
                "TARGET_SCHEMA_MISMATCH"
            );
        }
        for s in &m.sets {
            let target_meta = meta
                .iter()
                .find(|col| col.name == s.target)
                .ok_or_else(|| anyhow::anyhow!("TARGET_SCHEMA_MISMATCH"))?;
            ensure!(!target_meta.generated, "TARGET_SCHEMA_MISMATCH");
            match s.operation.as_str() {
                "replace" => {
                    let source = c.fields.iter().find(|f| f.column == s.source).unwrap();
                    let target = c.fields.iter().find(|f| f.column == s.target).unwrap();
                    ensure!(
                        source.kind == target.kind
                            && source.precision == target.precision
                            && source.scale == target.scale
                            && source.max_length == target.max_length,
                        "Incompatible replacement mapping"
                    );
                }
                "set_value" => {
                    if s.value.as_ref().is_some_and(serde_json::Value::is_null) {
                        ensure!(target_meta.nullable, "TARGET_SCHEMA_MISMATCH");
                    }
                }
                "current_timestamp" => {
                    ensure!(
                        matches!(
                            target_meta.logical(),
                            "date" | "datetime" | "datetime_offset"
                        ),
                        "TARGET_SCHEMA_MISMATCH"
                    );
                }
                _ => anyhow::bail!("Incompatible replacement mapping"),
            }
        }
    }
    Ok(())
}
pub fn check_values(c: &Config, meta: &[Meta], spool: &Spool, report: &mut Report) -> Result<()> {
    let map: HashMap<_, _> = meta.iter().map(|m| (m.name.as_str(), m)).collect();
    let text_scope: Vec<_> = c
        .replace_scope
        .iter()
        .filter_map(|filter| {
            let field = c
                .fields
                .iter()
                .position(|field| field.column == filter.column && field.kind == "string")?;
            Some((field, &filter.value))
        })
        .collect();
    for row in spool.rows()? {
        crate::check_cancelled()?;
        let row = row?;
        for &(index, expected) in &text_scope {
            if row.values[index].as_deref() != expected.as_str() {
                let mut detail = Detail::new(
                    "REPLACE_SCOPE_MISMATCH",
                    "Workbook contains rows outside the replacement scope",
                );
                detail.sheet = Some(row.sheet.clone());
                detail.row = Some(row.row);
                detail.column = Some(column(index));
                detail.field = Some(c.fields[index].column.clone());
                report.error(detail)?;
            }
        }
        for (i, (v, f)) in row.values.iter().zip(&c.fields).enumerate() {
            let Some(&m) = map.get(f.column.as_str()) else {
                continue;
            };
            let mut bad = v.is_none() && !m.nullable;
            if let Some(v) = v {
                if c.connection.driver == "mssql" && matches!(m.kind.as_str(), "varchar" | "char") {
                    bad |= !v.is_ascii();
                }
                if matches!(m.kind.as_str(), "nvarchar" | "nchar") {
                    bad |= m.length.is_some_and(|n| v.encode_utf16().count() > n);
                }
                if let Some(p) = m.time_precision
                    && let Some((_, fraction)) = v.split_once('.')
                {
                    let n = fraction.bytes().take_while(u8::is_ascii_digit).count();
                    bad |= n > p as usize;
                }
                if matches!(m.kind.as_str(), "datetime" | "smalldatetime") {
                    use chrono::Timelike;
                    let d = chrono::NaiveDateTime::parse_from_str(v, "%Y-%m-%d %H:%M:%S%.f")?;
                    let ns = d.nanosecond();
                    bad |= if m.kind == "datetime" {
                        v.as_str() < "1753-01-01"
                            || ns % 1_000_000 != 0
                            || !matches!(ns / 1_000_000 % 10, 0 | 3 | 7)
                    } else {
                        v.as_str() < "1900-01-01"
                            || v.as_str() >= "2079-06-07"
                            || d.second() != 0
                            || ns != 0
                    };
                }
            }
            if bad {
                let mut d = Detail::new(
                    "TARGET_SCHEMA_MISMATCH",
                    "Value would lose precision or violate target nullability/length",
                );
                d.sheet = Some(row.sheet.clone());
                d.row = Some(row.row);
                d.column = Some(column(i));
                d.field = Some(f.column.clone());
                report.error(d)?;
            }
        }
    }
    Ok(())
}
pub async fn apply(
    db: &mut Db,
    c: &Config,
    meta: &[Meta],
    spool: &Spool,
    keys: &[String],
    report: &mut Report,
    dry: bool,
) -> Result<()> {
    db.begin().await?;
    let outcome = apply_transaction(db, c, meta, spool, keys, report, dry).await;
    if outcome.is_err() || dry || report.summary.error_count > 0 {
        let _ = db.rollback().await;
        report.summary.inserted_rows = 0;
        report.summary.updated_rows = 0;
        report.summary.deleted_rows = 0;
    }
    outcome
}
async fn apply_transaction(
    db: &mut Db,
    c: &Config,
    meta: &[Meta],
    spool: &Spool,
    keys: &[String],
    report: &mut Report,
    dry: bool,
) -> Result<()> {
    let table = db.table(c.target.as_ref().unwrap());
    // Serialize target writers and protect metadata until commit. A later tuning
    // pass can narrow these locks after equivalent concurrency tests exist.
    if db.pg() {
        db.batch(&format!("LOCK TABLE {table} IN SHARE ROW EXCLUSIVE MODE"))
            .await?;
    } else {
        db.scalar(&format!(
            "SELECT {} FROM {table} WITH (TABLOCKX,HOLDLOCK)",
            db.text("COUNT(*)")
        ))
        .await?;
    }
    ensure!(
        db.inspect(c.target.as_ref().unwrap()).await? == meta,
        "Target metadata changed during validation"
    );
    if !keys.is_empty()
        && (c.operation != "mutate"
            || c.mutation
                .as_ref()
                .is_some_and(|m| m.matching.require_unique_index))
    {
        let indexes = db.unique_keys(c.target.as_ref().unwrap()).await?;
        ensure!(
            indexes
                .iter()
                .any(|ix| ix.len() == keys.len() && keys.iter().all(|k| ix.contains(k))),
            "Target unique key changed during validation"
        );
    }
    let stage = db.stage(c, meta, spool).await?;
    // Target collation/equality can differ from Rust string equality.
    if !keys.is_empty() {
        let key_sql = keys.iter().map(|k| db.q(k)).collect::<Vec<_>>().join(",");
        let duplicates=db.scalar(&format!("SELECT {} FROM (SELECT {key_sql} FROM {stage} GROUP BY {key_sql} HAVING COUNT(*)>1) d",db.text("COUNT(*)"))).await?;
        if duplicates > 0 {
            report.error(Detail::new(
                "DUPLICATE_SOURCE_KEY",
                "Duplicate keys under database comparison rules",
            ))?;
            return Ok(());
        }
        db.batch(&format!(
            "CREATE UNIQUE INDEX pc_stage_key ON {stage} ({key_sql})"
        ))
        .await?;
    }
    let fields = c
        .fields
        .iter()
        .map(|f| db.q(&f.column))
        .collect::<Vec<_>>()
        .join(",");
    if c.operation == "import" {
        if !c.replace_scope.is_empty() {
            ensure!(
                spool.count > 0,
                "Cannot replace a scope with an empty workbook"
            );
            let mut values = Vec::new();
            let mut clauses = Vec::new();
            for filter in &c.replace_scope {
                let column = meta.iter().find(|m| m.name == filter.column).unwrap();
                values.push(crate::export::value_string(&filter.value)?);
                let mut parameter = db.param(values.len());
                if db.pg() {
                    parameter.push_str("::text");
                }
                clauses.push(format!(
                    "{} = CAST({parameter} AS {})",
                    db.q(&column.name),
                    column.sql_type(db.pg())?
                ));
            }
            let scope = clauses.join(" AND ");
            let outside = db
                .scalar_bound(
                    &format!(
                        "SELECT {} FROM {stage} WHERE CASE WHEN {scope} THEN 0 ELSE 1 END = 1",
                        db.text("COUNT(*)")
                    ),
                    &values,
                )
                .await?;
            if outside > 0 {
                report.error(Detail::new(
                    "REPLACE_SCOPE_MISMATCH",
                    "Workbook contains rows outside the replacement scope",
                ))?;
                return Ok(());
            }
            if dry {
                return Ok(());
            }
            report.summary.deleted_rows = db
                .execute_bound(&format!("DELETE FROM {table} WHERE {scope}"), &values)
                .await?;
        }
        if dry {
            if !keys.is_empty() {
                let join = keys
                    .iter()
                    .map(|k| format!("t.{0}=s.{0}", db.q(k)))
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let collisions = db.scalar(&format!("SELECT {} FROM {stage} s WHERE EXISTS (SELECT 1 FROM {table} t WHERE {join})", db.text("COUNT(*)"))).await?;
                if collisions > 0 {
                    report.error(Detail::new(
                        "TARGET_KEY_COLLISION",
                        "Source keys already exist in target",
                    ))?;
                }
            }
            return Ok(());
        }
        let n = db
            .execute(&format!(
                "INSERT INTO {table} ({fields}) SELECT {fields} FROM {stage}"
            ))
            .await?;
        ensure!(n == spool.count, "Insert count mismatch");
        report.summary.inserted_rows = n;
    } else {
        let m = c.mutation.as_ref().unwrap();
        let pg = db.pg();
        let push_param = |params: &mut Vec<String>, val: String, sql_type: &str| -> String {
            params.push(val);
            let mut p = if pg {
                format!("${}", params.len())
            } else {
                format!("@P{}", params.len())
            };
            if pg {
                p.push_str("::text");
            }
            format!("CAST({p} AS {sql_type})")
        };
        let build_scope = |params: &mut Vec<String>| -> Result<Vec<String>> {
            let mut clauses = Vec::new();
            for filter in &m.scope {
                let col = meta.iter().find(|col| col.name == filter.column).unwrap();
                let expr = push_param(
                    params,
                    crate::export::value_string(&filter.value)?,
                    &col.sql_type(pg)?,
                );
                let qcol = if pg {
                    format!("\"{}\"", col.name.replace('"', "\"\""))
                } else {
                    format!("[{}]", col.name.replace(']', "]]"))
                };
                clauses.push(format!("t.{qcol} = {expr}"));
            }
            Ok(clauses)
        };
        let delete_flag_info = if let Some(df) = &m.delete_flag {
            let f = c.fields.iter().find(|f| f.column == df.column).unwrap();
            Some((
                db.q(&df.column),
                crate::export::value_string(&df.value)?,
                field_sql_type(f, meta, pg)?,
            ))
        } else {
            None
        };
        let join = keys
            .iter()
            .map(|k| format!("t.{0}=s.{0}", db.q(k)))
            .collect::<Vec<_>>()
            .join(" AND ");
        let locked = if pg {
            format!("{table} t")
        } else {
            format!("{table} t WITH (UPDLOCK,HOLDLOCK)")
        };
        let mut scope_params = Vec::new();
        let scope_clauses = build_scope(&mut scope_params)?;
        let join_scoped = if scope_clauses.is_empty() {
            join.clone()
        } else {
            format!("{join} AND {}", scope_clauses.join(" AND "))
        };
        let missing = format!("NOT EXISTS (SELECT 1 FROM {locked} WHERE {join_scoped})");
        report.summary.missing_rows = db
            .scalar_bound(
                &format!(
                    "SELECT {} FROM {stage} s WHERE {missing}",
                    db.text("COUNT(*)")
                ),
                &scope_params,
            )
            .await?;
        report.summary.matched_rows = spool.count - report.summary.missing_rows;
        if m.mode == "update_strict" && report.summary.missing_rows > 0 {
            let sql = format!(
                "SELECT {},{},{} FROM {stage} s WHERE {missing} ORDER BY __excel_sheet,__excel_row",
                db.text("__excel_sheet"),
                db.text("__excel_row"),
                keys.iter()
                    .map(|k| db.text(&format!("s.{}", db.q(k))))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            db.stream(&sql, &scope_params, |r| {
                let mut d = Detail::new("KEY_NOT_FOUND", "Key not found in target");
                d.sheet = r[0].clone();
                d.row = Some(r[1].as_ref().unwrap().parse()?);
                d.key = Some(
                    keys.iter()
                        .enumerate()
                        .map(|(i, k)| {
                            let sensitive =
                                c.fields.iter().find(|f| f.column == *k).unwrap().sensitive;
                            (
                                k.clone(),
                                if sensitive {
                                    Some("[REDACTED]".into())
                                } else {
                                    r[i + 2].clone().map(|s| s.chars().take(80).collect())
                                },
                            )
                        })
                        .collect(),
                );
                d.column = Some(column(
                    c.fields.iter().position(|f| f.column == keys[0]).unwrap(),
                ));
                report.error(d)
            })
            .await?;
            return Ok(());
        }
        if !m.matching.require_unique_index {
            let t_keys = keys
                .iter()
                .map(|k| format!("t.{}", db.q(k)))
                .collect::<Vec<_>>()
                .join(",");
            let where_scope = if scope_clauses.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", scope_clauses.join(" AND "))
            };
            let ambiguous = db
                .scalar_bound(
                    &format!(
                        "SELECT {} FROM (SELECT {t_keys} FROM {table} t JOIN {stage} s ON {join}{where_scope} GROUP BY {t_keys} HAVING COUNT(*)>1) d",
                        db.text("COUNT(*)")
                    ),
                    &scope_params,
                )
                .await?;
            if ambiguous > 0 {
                report.error(Detail::new(
                    "AMBIGUOUS_TARGET_KEY",
                    "Multiple target rows match a single source key",
                ))?;
                return Ok(());
            }
        }
        let to_delete = if let Some((df_col, df_val, df_type)) = delete_flag_info.as_ref() {
            let mut params = Vec::new();
            let sc = build_scope(&mut params)?;
            let j = if sc.is_empty() {
                join.clone()
            } else {
                format!("{join} AND {}", sc.join(" AND "))
            };
            let df_expr = push_param(&mut params, df_val.clone(), df_type);
            db.scalar_bound(
                &format!(
                    "SELECT {} FROM {stage} s WHERE s.{df_col} = {df_expr} AND EXISTS (SELECT 1 FROM {locked} WHERE {j})",
                    db.text("COUNT(*)")
                ),
                &params,
            )
            .await?
        } else {
            0
        };
        let to_update = report.summary.matched_rows - to_delete;
        if dry {
            return Ok(());
        }
        if to_delete > 0 {
            let (df_col, df_val, df_type) = delete_flag_info.as_ref().unwrap();
            let mut params = Vec::new();
            let mut conds = build_scope(&mut params)?;
            let df_expr = push_param(&mut params, df_val.clone(), df_type);
            conds.push(format!("s.{df_col} = {df_expr}"));
            let sql = if pg {
                format!(
                    "DELETE FROM {table} t USING {stage} s WHERE {join} AND {}",
                    conds.join(" AND ")
                )
            } else {
                format!(
                    "DELETE t FROM {table} t WITH (UPDLOCK,HOLDLOCK) JOIN {stage} s ON {join} WHERE {}",
                    conds.join(" AND ")
                )
            };
            let n = db.execute_bound(&sql, &params).await?;
            ensure!(n == to_delete, "Delete count mismatch");
            report.summary.deleted_rows = n;
        }
        if to_update > 0 {
            let mut params = Vec::new();
            let mut set_clauses = Vec::with_capacity(m.sets.len());
            for s in &m.sets {
                let col = db.q(&s.target);
                match s.operation.as_str() {
                    "replace" => set_clauses.push(format!("{col}=s.{}", db.q(&s.source))),
                    "current_timestamp" => set_clauses.push(format!("{col}=CURRENT_TIMESTAMP")),
                    "set_value" => {
                        let val = s.value.as_ref().unwrap();
                        if val.is_null() {
                            set_clauses.push(format!("{col}=NULL"));
                        } else {
                            let target_meta = meta.iter().find(|cm| cm.name == s.target).unwrap();
                            let expr = push_param(
                                &mut params,
                                crate::export::value_string(val)?,
                                &target_meta.sql_type(pg)?,
                            );
                            set_clauses.push(format!("{col}={expr}"));
                        }
                    }
                    _ => unreachable!(),
                }
            }
            let mut conds = build_scope(&mut params)?;
            if let Some((df_col, df_val, df_type)) = delete_flag_info.as_ref() {
                let df_expr = push_param(&mut params, df_val.clone(), df_type);
                conds.push(format!("(s.{df_col} IS NULL OR s.{df_col} <> {df_expr})"));
            }
            let sets = set_clauses.join(",");
            let sql = if pg {
                let extra = if conds.is_empty() {
                    String::new()
                } else {
                    format!(" AND {}", conds.join(" AND "))
                };
                format!("UPDATE {table} t SET {sets} FROM {stage} s WHERE {join}{extra}")
            } else {
                let extra = if conds.is_empty() {
                    String::new()
                } else {
                    format!(" WHERE {}", conds.join(" AND "))
                };
                format!(
                    "UPDATE t SET {sets} FROM {table} t WITH (UPDLOCK,HOLDLOCK) JOIN {stage} s ON {join}{extra}"
                )
            };
            let n = db.execute_bound(&sql, &params).await?;
            ensure!(n == to_update, "Update count mismatch");
            report.summary.updated_rows = n;
        }
        if m.mode == "upsert" {
            let mut params = Vec::new();
            let sc = build_scope(&mut params)?;
            let j = if sc.is_empty() {
                join.clone()
            } else {
                format!("{join} AND {}", sc.join(" AND "))
            };
            let mut cond = format!("NOT EXISTS (SELECT 1 FROM {locked} WHERE {j})");
            if let Some((df_col, df_val, df_type)) = delete_flag_info.as_ref() {
                let df_expr = push_param(&mut params, df_val.clone(), df_type);
                cond.push_str(&format!(
                    " AND (s.{df_col} IS NULL OR s.{df_col} <> {df_expr})"
                ));
            }
            let expected_insert = db
                .scalar_bound(
                    &format!("SELECT {} FROM {stage} s WHERE {cond}", db.text("COUNT(*)")),
                    &params,
                )
                .await?;
            if expected_insert > 0 {
                let cols = m
                    .insert_columns
                    .iter()
                    .map(|n| db.q(n))
                    .collect::<Vec<_>>()
                    .join(",");
                let select = m
                    .insert_columns
                    .iter()
                    .map(|n| format!("s.{}", db.q(n)))
                    .collect::<Vec<_>>()
                    .join(",");
                let n = db
                    .execute_bound(
                        &format!(
                            "INSERT INTO {table} ({cols}) SELECT {select} FROM {stage} s WHERE {cond}"
                        ),
                        &params,
                    )
                    .await?;
                ensure!(n == expected_insert, "Insert count mismatch");
                report.summary.inserted_rows = n;
            }
        }
    }
    if !dry {
        crate::check_cancelled()?;
        if let Err(e) = db.commit().await {
            let server_rejected = e
                .downcast_ref::<tokio_postgres::Error>()
                .and_then(|e| e.code())
                .is_some()
                || e.downcast_ref::<tiberius::error::Error>()
                    .is_some_and(|e| matches!(e, tiberius::error::Error::Server(_)));
            if !server_rejected {
                report.commit_outcome = "unknown".into();
            }
            return Err(e);
        }
        report.committed = true;
        report.commit_outcome = "confirmed".into();
    }
    Ok(())
}
