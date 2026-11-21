use crate::{
    config::{Config, Sheet, Table},
    db::{Db, Meta},
    report::Report,
};
use anyhow::{Result, ensure};
use rust_xlsxwriter::{Color, Format, Workbook, Worksheet};
use std::{collections::HashMap, path::Path};
fn start(book: &mut Workbook, s: &Sheet, index: u32) -> Result<Worksheet> {
    let mut sheet = book.new_worksheet_with_constant_memory()?;
    let name = if index == 1 {
        s.name.clone()
    } else {
        format!("{}_{index}", s.name)
    };
    sheet.set_name(name)?;
    let mut format = Format::new()
        .set_background_color(Color::RGB(u32::from_str_radix(
            &s.header.background[1..],
            16,
        )?))
        .set_font_color(Color::RGB(u32::from_str_radix(
            &s.header.font_color[1..],
            16,
        )?));
    if s.header.bold {
        format = format.set_bold();
    }
    for (i, c) in s.columns.iter().enumerate() {
        sheet.set_column_width(i as u16, c.width)?;
        sheet.write_string_with_format(s.header.row - 1, i as u16, &c.excel_header, &format)?;
    }
    if s.header.freeze {
        sheet.set_freeze_panes(s.header.row, 0)?;
    }
    Ok(sheet)
}
fn finish(sheet: &mut Worksheet, s: &Sheet, rows: u32) -> Result<()> {
    if s.header.autofilter {
        sheet.autofilter(
            s.header.row - 1,
            0,
            s.header.row - 1 + rows,
            (s.columns.len() - 1) as u16,
        )?;
    }
    Ok(())
}
fn value_string(v: &serde_json::Value) -> Result<String> {
    Ok(match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => if *b { "1" } else { "0" }.into(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => anyhow::bail!("Filter value must be scalar and non-null"),
    })
}
pub fn select(db: &Db, s: &Sheet, meta: &[Meta]) -> Result<(String, Vec<String>)> {
    let map: HashMap<_, _> = meta.iter().map(|m| (m.name.as_str(), m)).collect();
    let get = |name: &str| -> Result<&Meta> {
        map.get(name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("TARGET_SCHEMA_MISMATCH"))
    };
    let cols = s
        .columns
        .iter()
        .map(|c| {
            get(&c.db_column)?.sql_type(db.pg())?;
            let m = get(&c.db_column)?;
            if db.pg() && m.logical() == "datetime_offset" {
                Ok(format!(
                    "to_char({} AT TIME ZONE 'UTC', 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"')",
                    db.q(&c.db_column)
                ))
            } else {
                Ok(db.text(&db.q(&c.db_column)))
            }
        })
        .collect::<Result<Vec<_>>>()?
        .join(",");
    let mut params = vec![];
    let mut conditions = vec![];
    for f in &s.source.filters {
        let m = get(&f.column)?;
        let kind = m.sql_type(db.pg())?;
        let vals = if matches!(f.op.as_str(), "in" | "between") {
            f.value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("Filter requires array"))?
                .clone()
        } else {
            vec![f.value.clone()]
        };
        ensure!(
            !vals.is_empty() && vals.len() <= 1000 && (f.op != "between" || vals.len() == 2),
            "Invalid filter arity"
        );
        let mut holders = vec![];
        for v in vals {
            params.push(value_string(&v)?);
            holders.push(format!(
                "CAST({} AS {kind})",
                if db.pg() {
                    format!("{}::text", db.param(params.len()))
                } else {
                    db.param(params.len())
                }
            ));
        }
        let q = db.q(&f.column);
        conditions.push(match f.op.as_str() {
            "in" => format!("{q} IN ({})", holders.join(",")),
            "between" => format!("{q} BETWEEN {} AND {}", holders[0], holders[1]),
            _ => format!(
                "{q} {} {}",
                match f.op.as_str() {
                    "eq" => "=",
                    "gte" => ">=",
                    _ => "<=",
                },
                holders[0]
            ),
        });
    }
    let mut sql = format!(
        "SELECT {cols} FROM {}",
        db.table(&Table {
            schema: s.source.schema.clone(),
            table: s.source.table.clone()
        })
    );
    if !conditions.is_empty() {
        sql.push_str(&format!(" WHERE {}", conditions.join(" AND ")));
    }
    if !s.source.order_by.is_empty() {
        let order = s
            .source
            .order_by
            .iter()
            .map(|o| {
                get(&o.column)?;
                Ok(format!("{} {}", db.q(&o.column), o.direction))
            })
            .collect::<Result<Vec<_>>>()?;
        sql.push_str(&format!(" ORDER BY {}", order.join(",")));
    }
    Ok((sql, params))
}
pub async fn export(
    db: &mut Db,
    c: &Config,
    output: &Path,
    report: &mut Report,
    dry: bool,
) -> Result<()> {
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;
    std::fs::create_dir_all(&c.execution.temp_dir)?;
    let temp = tempfile::Builder::new()
        .suffix(".partial.xlsx")
        .tempfile_in(parent)?;
    let mut book = Workbook::new();
    book.set_tempdir(&c.execution.temp_dir)?;
    let options = c.workbook.as_ref().unwrap();
    db.begin().await?;
    let result = async {
        for s in &c.sheets {
            let meta = db
                .inspect(&Table {
                    schema: s.source.schema.clone(),
                    table: s.source.table.clone(),
                })
                .await?;
            let (sql, params) = select(db, s, &meta)?;
            let kinds: Vec<_> = s
                .columns
                .iter()
                .map(|c| {
                    meta.iter()
                        .find(|m| m.name == c.db_column)
                        .unwrap()
                        .logical()
                })
                .collect();
            let formats: Vec<_> = s
                .columns
                .iter()
                .map(|c| {
                    Format::new().set_num_format(if c.format == "text" { "@" } else { &c.format })
                })
                .collect();
            let mut index = 1u32;
            let mut sheet = start(&mut book, s, index)?;
            let mut rows = 0u32;
            db.stream(&sql, &params, |values| {
                crate::check_cancelled()?;
                if rows == options.max_data_rows_per_sheet {
                    ensure!(options.auto_split_sheets, "SHEET_ROW_LIMIT");
                    finish(&mut sheet, s, rows)?;
                    index += 1;
                    let next = start(&mut book, s, index)?;
                    book.push_worksheet(std::mem::replace(&mut sheet, next));
                    rows = 0;
                }
                for (i, v) in values.iter().enumerate() {
                    if let Some(v) = v {
                        let r = s.header.row + rows;
                        let col = i as u16;
                        let conf = &s.columns[i];
                        match kinds[i] {
                            "boolean" if conf.format != "text" => {
                                sheet.write_boolean(
                                    r,
                                    col,
                                    matches!(v.as_str(), "1" | "true" | "t"),
                                )?;
                            }
                            "int16" | "int32" | "float64" if conf.format != "text" => {
                                let n = v.parse::<f64>()?;
                                ensure!(n.is_finite(), "NON_FINITE_NUMBER");
                                sheet.write_number_with_format(r, col, n, &formats[i])?;
                            }
                            "date" if conf.format != "text" => {
                                let d = chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d")?;
                                sheet.write_datetime_with_format(r, col, d, &formats[i])?;
                            }
                            "datetime" if conf.format != "text" => {
                                let d = chrono::NaiveDateTime::parse_from_str(
                                    v,
                                    "%Y-%m-%d %H:%M:%S%.f",
                                )
                                .or_else(|_| {
                                    chrono::NaiveDateTime::parse_from_str(v, "%Y-%m-%dT%H:%M:%S%.f")
                                })?;
                                sheet.write_datetime_with_format(r, col, d, &formats[i])?;
                            }
                            "time" if conf.format != "text" => {
                                let t = chrono::NaiveTime::parse_from_str(v, "%H:%M:%S%.f")?;
                                sheet.write_datetime_with_format(r, col, t, &formats[i])?;
                            }
                            _ => {
                                sheet.write_string_with_format(r, col, v, &formats[i])?;
                            }
                        }
                    }
                }
                rows += 1;
                report.summary.exported_rows += 1;
                Ok(())
            })
            .await?;
            finish(&mut sheet, s, rows)?;
            book.push_worksheet(sheet);
        }
        if !dry {
            book.save(temp.path())?;
            temp.as_file().sync_all()?;
            crate::excel::guard(temp.path(), c.execution.memory_limit_mb)?;
        }
        Ok::<(), anyhow::Error>(())
    }
    .await;
    let rollback = db.rollback().await;
    result?;
    rollback?;
    if !dry {
        crate::check_cancelled()?;
        temp.persist(output).map_err(|e| e.error)?;
    }
    Ok(())
}
