use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::Path};

fn yes() -> bool {
    true
}
fn one() -> u32 {
    1
}
fn two() -> u32 {
    2
}
fn samples() -> usize {
    20
}
fn memory() -> u64 {
    512
}
fn batch() -> usize {
    10000
}
fn temp() -> String {
    "./tmp".into()
}
fn rows() -> u32 {
    1048575
}
fn exact() -> String {
    "exact_order".into()
}
fn xlsx() -> String {
    "xlsx".into()
}
fn reject() -> String {
    "reject".into()
}
fn loss() -> String {
    "reject_loss".into()
}
fn jsonl() -> String {
    "jsonl".into()
}
fn serial() -> String {
    "serializable".into()
}
fn constant() -> String {
    "constant_memory".into()
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: String,
    pub name: String,
    pub operation: String,
    pub connection: Connection,
    pub target: Option<Table>,
    pub excel: Option<Excel>,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub validation: Validation,
    #[serde(default)]
    pub execution: Execution,
    #[serde(default = "reject")]
    pub conflict_policy: String,
    pub mutation: Option<Mutation>,
    pub workbook: Option<Workbook>,
    #[serde(default)]
    pub sheets: Vec<Sheet>,
    pub input: Option<String>,
    pub output: Option<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connection {
    pub driver: String,
    pub dsn_env: String,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Table {
    pub schema: String,
    pub table: String,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Excel {
    #[serde(default = "xlsx")]
    pub format: String,
    pub sheet: Option<String>,
    pub sheets: Option<SheetSelector>,
    #[serde(default = "one")]
    pub header_row: u32,
    #[serde(default = "two")]
    pub first_data_row: u32,
    #[serde(default = "exact")]
    pub header_policy: String,
    #[serde(default = "yes")]
    pub reject_unknown_columns: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SheetSelector {
    pub mode: String,
    pub names: Vec<String>,
    pub same_layout: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Field {
    pub excel: String,
    pub column: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub required: bool,
    pub max_length: Option<usize>,
    pub precision: Option<u32>,
    pub scale: Option<u32>,
    pub excel_storage: Option<String>,
    pub date_format: Option<String>,
    #[serde(default)]
    pub trim: bool,
    #[serde(default)]
    pub empty_as_null: bool,
    #[serde(default)]
    pub null_tokens: Vec<String>,
    #[serde(default)]
    pub sensitive: bool,
    #[serde(rename = "enum", default)]
    pub choices: Vec<String>,
    pub regex: Option<String>,
    pub min: Option<String>,
    pub max: Option<String>,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Validation {
    #[serde(default = "yes")]
    pub strict: bool,
    #[serde(default = "yes")]
    pub reject_formulas: bool,
    #[serde(default = "yes")]
    pub reject_duplicate_keys: bool,
    #[serde(default = "samples")]
    pub max_error_samples: usize,
    #[serde(default = "loss")]
    pub decimal_conversion: String,
}
impl Default for Validation {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Execution {
    #[serde(default = "yes")]
    pub atomic: bool,
    #[serde(default = "memory")]
    pub memory_limit_mb: u64,
    #[serde(default = "batch")]
    pub batch_rows: usize,
    #[serde(default = "batch")]
    pub fetch_rows: usize,
    #[serde(default = "temp")]
    pub temp_dir: String,
    #[serde(default = "jsonl")]
    pub error_report_format: String,
}
impl Default for Execution {
    fn default() -> Self {
        serde_json::from_str("{}").unwrap()
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mutation {
    pub mode: String,
    #[serde(rename = "match")]
    pub matching: Match,
    pub sets: Vec<Set>,
    #[serde(default)]
    pub insert_columns: Vec<String>,
    #[serde(default = "serial")]
    pub isolation: String,
    #[serde(default)]
    pub max_serialization_retries: u8,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Match {
    pub keys: Vec<String>,
    #[serde(default)]
    pub must_match_all: bool,
    #[serde(default = "yes")]
    pub require_unique_index: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Set {
    pub target: String,
    pub source: String,
    pub operation: String,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Workbook {
    pub filename: Option<String>,
    #[serde(default = "xlsx")]
    pub format: String,
    #[serde(default = "constant")]
    pub writer_mode: String,
    #[serde(default = "yes")]
    pub auto_split_sheets: bool,
    #[serde(default = "rows")]
    pub max_data_rows_per_sheet: u32,
    #[serde(default = "yes")]
    pub safe_text_mode: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    pub name: String,
    pub source: Source,
    pub header: Header,
    pub columns: Vec<Column>,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub schema: String,
    pub table: String,
    #[serde(default)]
    pub filters: Vec<Filter>,
    #[serde(default)]
    pub order_by: Vec<Order>,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Filter {
    pub column: String,
    pub op: String,
    pub value: serde_json::Value,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub column: String,
    pub direction: String,
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Header {
    #[serde(default = "one")]
    pub row: u32,
    pub background: String,
    pub font_color: String,
    #[serde(default)]
    pub bold: bool,
    #[serde(default)]
    pub freeze: bool,
    #[serde(default)]
    pub autofilter: bool,
    #[serde(default)]
    pub layout: Vec<HeaderCell>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HeaderCell {
    pub row: u32,
    pub column: u16,
    pub last_row: Option<u32>,
    pub last_column: Option<u16>,
    pub text: String,
    pub background: Option<String>,
    pub font_color: Option<String>,
    pub bold: Option<bool>,
}

impl HeaderCell {
    pub fn end(&self) -> (u32, u16) {
        (
            self.last_row.unwrap_or(self.row),
            self.last_column.unwrap_or(self.column),
        )
    }
}
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Column {
    pub db_column: String,
    pub excel_header: String,
    pub width: f64,
    pub format: String,
    #[serde(default)]
    pub decimal_as_text: bool,
}
pub fn identifier(s: &str) -> Result<()> {
    ensure!(
        !s.is_empty()
            && s.len() <= 63
            && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            && !s.starts_with("__")
            && !s.as_bytes()[0].is_ascii_digit(),
        "Invalid identifier"
    );
    Ok(())
}
fn unique<'a>(v: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = HashSet::new();
    v.into_iter().all(|x| seen.insert(x))
}

fn check_color(color: &str) -> Result<()> {
    ensure!(
        color.len() == 7
            && color.starts_with('#')
            && color.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit),
        "Invalid color; expected #RRGGBB"
    );
    Ok(())
}

fn check_header(sheet: &Sheet) -> Result<()> {
    check_color(&sheet.header.background)?;
    check_color(&sheet.header.font_color)?;
    let mut used = HashSet::new();
    let mut cells = 0u64;
    for cell in &sheet.header.layout {
        let (last_row, last_column) = cell.end();
        ensure!(
            cell.row > 0
                && cell.row <= last_row
                && last_row < sheet.header.row
                && cell.column > 0
                && cell.column <= last_column
                && usize::from(last_column) <= sheet.columns.len(),
            "Header layout ranges must be within the exported columns and above header.row"
        );
        ensure!(
            cell.text.encode_utf16().count() <= 32767,
            "Header text exceeds Excel cell limit"
        );
        for color in [&cell.background, &cell.font_color].into_iter().flatten() {
            check_color(color)?;
        }
        cells += u64::from(last_row - cell.row + 1) * u64::from(last_column - cell.column + 1);
        ensure!(
            cells <= 32768,
            "Header layout exceeds 32768 formatted cells"
        );
        for row in cell.row..=last_row {
            for column in cell.column..=last_column {
                ensure!(
                    used.insert((row, column)),
                    "Overlapping header layout ranges"
                );
            }
        }
    }
    Ok(())
}
impl Config {
    pub fn read(path: &Path) -> Result<Self> {
        let c: Self = serde_json::from_reader(std::fs::File::open(path)?)?;
        c.check()?;
        Ok(c)
    }
    pub fn check(&self) -> Result<()> {
        ensure!(self.schema_version == "1.0", "Unsupported schema_version");
        ensure!(
            matches!(self.operation.as_str(), "import" | "export" | "mutate"),
            "Unsupported operation"
        );
        ensure!(
            matches!(self.connection.driver.as_str(), "postgres" | "mssql")
                && !self.connection.dsn_env.is_empty(),
            "Invalid connection"
        );
        ensure!(
            self.execution.atomic
                && self.execution.memory_limit_mb >= 64
                && self.execution.memory_limit_mb <= 16384
                && (1..=100000).contains(&self.execution.batch_rows)
                && (1..=100000).contains(&self.execution.fetch_rows),
            "Invalid execution limits"
        );
        ensure!(
            self.validation.strict
                && self.validation.reject_formulas
                && self.validation.reject_duplicate_keys
                && self.validation.decimal_conversion == "reject_loss"
                && self.validation.max_error_samples <= 1000
                && self.conflict_policy == "reject"
                && self.execution.error_report_format == "jsonl",
            "Only strict atomic policies supported"
        );
        if self.operation == "export" {
            ensure!(
                self.target.is_none()
                    && self.excel.is_none()
                    && self.fields.is_empty()
                    && self.mutation.is_none(),
                "Export has import-only properties"
            );
            let w = self
                .workbook
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("workbook required"))?;
            ensure!(
                w.format == "xlsx"
                    && w.writer_mode == "constant_memory"
                    && w.safe_text_mode
                    && (1..=1048575).contains(&w.max_data_rows_per_sheet),
                "Invalid workbook options"
            );
            ensure!(
                !self.sheets.is_empty()
                    && unique(
                        self.sheets
                            .iter()
                            .map(|s| s.name.to_lowercase())
                            .collect::<Vec<_>>()
                            .iter()
                            .map(String::as_str)
                    ),
                "Empty or duplicate sheets"
            );
            for s in &self.sheets {
                identifier(&s.source.schema)?;
                identifier(&s.source.table)?;
                ensure!(
                    !s.name.is_empty()
                        && s.name.chars().count() <= 25
                        && !s.name.contains(['[', ']', ':', '*', '?', '/', '\\'])
                        && !s.name.starts_with('\'')
                        && !s.name.ends_with('\''),
                    "Invalid sheet name (max 25 for split suffix)"
                );
                ensure!(
                    s.header.row > 0
                        && s.header.row < 1048576
                        && !s.columns.is_empty()
                        && s.columns.len() <= 16384,
                    "Invalid sheet dimensions"
                );
                check_header(s)?;
                for c in &s.columns {
                    identifier(&c.db_column)?;
                    ensure!(c.width > 0.0 && c.width <= 255.0, "Invalid width");
                }
                for f in &s.source.filters {
                    identifier(&f.column)?;
                    ensure!(
                        matches!(f.op.as_str(), "eq" | "gte" | "lte" | "between" | "in"),
                        "Unsupported filter"
                    );
                }
                for o in &s.source.order_by {
                    identifier(&o.column)?;
                    ensure!(
                        matches!(o.direction.as_str(), "asc" | "desc"),
                        "Invalid direction"
                    );
                }
            }
        } else {
            ensure!(
                self.workbook.is_none() && self.sheets.is_empty(),
                "Unexpected export properties"
            );
            let t = self
                .target
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("target required"))?;
            identifier(&t.schema)?;
            identifier(&t.table)?;
            let e = self
                .excel
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("excel required"))?;
            ensure!(
                e.format == "xlsx"
                    && e.header_policy == "exact_order"
                    && e.reject_unknown_columns
                    && e.header_row > 0
                    && e.first_data_row > e.header_row
                    && e.first_data_row <= 1048576,
                "Invalid Excel options"
            );
            ensure!(
                e.sheet.is_some() != e.sheets.is_some(),
                "Provide sheet or sheets, exclusively"
            );
            if let Some(s) = &e.sheets {
                ensure!(
                    s.mode == "named"
                        && s.same_layout
                        && !s.names.is_empty()
                        && unique(s.names.iter().map(String::as_str)),
                    "Invalid sheet selector"
                );
            }
            ensure!(
                !self.fields.is_empty()
                    && self.fields.len() <= 16384
                    && unique(self.fields.iter().map(|f| f.column.as_str()))
                    && unique(self.fields.iter().map(|f| f.excel.as_str())),
                "Empty or duplicate fields"
            );
            for f in &self.fields {
                identifier(&f.column)?;
                ensure!(
                    matches!(
                        f.kind.as_str(),
                        "string"
                            | "int16"
                            | "int32"
                            | "int64"
                            | "decimal"
                            | "float64"
                            | "boolean"
                            | "date"
                            | "time"
                            | "datetime"
                            | "datetime_offset"
                            | "uuid"
                    ),
                    "Unknown logical type"
                );
                ensure!(
                    f.excel_storage.as_deref().is_none_or(|s| s == "text"),
                    "Unsupported excel_storage"
                );
                if f.kind == "decimal" {
                    ensure!(
                        f.precision.is_some_and(|p| (1..=38).contains(&p))
                            && f.scale.is_some_and(|s| s <= f.precision.unwrap()),
                        "Invalid decimal precision/scale"
                    );
                }
                if let Some(r) = &f.regex {
                    regex::Regex::new(r)?;
                }
                ensure!(
                    f.min.is_none() && f.max.is_none()
                        || matches!(
                            f.kind.as_str(),
                            "int16" | "int32" | "int64" | "decimal" | "float64"
                        ),
                    "Ranges require numeric field"
                );
            }
            if self.operation == "mutate" {
                let m = self
                    .mutation
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("mutation required"))?;
                ensure!(
                    matches!(m.mode.as_str(), "update_strict" | "upsert")
                        && m.isolation == "serializable"
                        && m.max_serialization_retries <= 5
                        && m.matching.require_unique_index
                        && !m.matching.keys.is_empty()
                        && !m.sets.is_empty(),
                    "Invalid mutation policy"
                );
                ensure!(
                    m.mode != "update_strict" || m.matching.must_match_all,
                    "update_strict requires must_match_all"
                );
                let has = |n: &str| self.fields.iter().any(|f| f.column == n);
                ensure!(
                    unique(m.matching.keys.iter().map(String::as_str))
                        && unique(m.sets.iter().map(|s| s.target.as_str()))
                        && unique(m.insert_columns.iter().map(String::as_str)),
                    "Duplicate mutation mapping"
                );
                for k in &m.matching.keys {
                    ensure!(
                        self.fields.iter().any(|f| f.column == *k && f.required),
                        "Key must be required source field"
                    );
                }
                for s in &m.sets {
                    ensure!(
                        has(&s.source)
                            && has(&s.target)
                            && s.operation == "replace"
                            && !m.matching.keys.contains(&s.target),
                        "Invalid update mapping"
                    );
                }
                for n in &m.insert_columns {
                    ensure!(has(n), "Unknown insert column");
                }
                ensure!(
                    m.mode != "upsert"
                        || m.matching.keys.iter().all(|k| m.insert_columns.contains(k)),
                    "Upsert insert_columns must contain keys"
                );
            } else {
                ensure!(self.mutation.is_none(), "Import cannot contain mutation");
            }
        }
        Ok(())
    }
}
