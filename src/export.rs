use crate::{
    config::{Config, Sheet, Table},
    db::{Db, Meta},
    report::Report,
};
use anyhow::{Result, ensure};
use rust_xlsxwriter::{Color, Format, FormatAlign, Workbook, Worksheet};
use std::{collections::HashMap, path::Path};

fn header_format(background: &str, font_color: &str, bold: bool) -> Result<Format> {
    let mut format = Format::new()
        .set_background_color(Color::RGB(u32::from_str_radix(&background[1..], 16)?))
        .set_font_color(Color::RGB(u32::from_str_radix(&font_color[1..], 16)?));
    if bold {
        format = format.set_bold();
    }
    Ok(format)
}

fn start(book: &mut Workbook, s: &Sheet, index: u32) -> Result<Worksheet> {
    let mut sheet = book.new_worksheet_with_constant_memory()?;
    let name = if index == 1 {
        s.name.clone()
    } else {
        format!("{}_{index}", s.name)
    };
    sheet.set_name(name)?;
    let header = &s.header;
    let format = header_format(&header.background, &header.font_color, header.bold)?;
    let mut layout: Vec<_> = header.layout.iter().collect();
    layout.sort_unstable_by_key(|cell| (cell.row, cell.column));
    for cell in layout {
        let (last_row, last_column) = cell.end();
        let style = header_format(
            cell.background.as_deref().unwrap_or(&header.background),
            cell.font_color.as_deref().unwrap_or(&header.font_color),
            cell.bold.unwrap_or(header.bold),
        )?;
        if (last_row, last_column) == (cell.row, cell.column) {
            sheet.write_string_with_format(cell.row - 1, cell.column - 1, &cell.text, &style)?;
        } else {
            let style = style
                .set_align(FormatAlign::Center)
                .set_align(FormatAlign::VerticalCenter);
            sheet.merge_range(
                cell.row - 1,
                cell.column - 1,
                last_row - 1,
                last_column - 1,
                &cell.text,
                &style,
            )?;
        }
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
            let rows_per_sheet = options.max_data_rows_per_sheet.min(1048576 - s.header.row);
            db.stream(&sql, &params, |values| {
                crate::check_cancelled()?;
                if rows == rows_per_sheet {
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
            crate::excel::guard_export(temp.path(), c.execution.memory_limit_mb)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use calamine::{Data, Reader, Xlsx, open_workbook};
    use std::{fs::File, io::Read};

    fn config(header_row: u32) -> Config {
        let mut config: Config =
            serde_json::from_str(include_str!("../examples/export-finance.json")).unwrap();
        config.sheets.truncate(1);
        config.sheets[0].header.row = header_row;
        config.sheets[0].header.layout = serde_json::from_value(serde_json::json!([
            {"row": header_row - 1, "column": 4, "text": "Unit", "background": "#FFF0CB"},
            {"row": 1, "column": 1, "last_row": header_row - 1, "last_column": 3,
             "text": "Project 1", "background": "#245A81", "font_color": "#FFFFFF"}
        ]))
        .unwrap();
        config
    }

    #[test]
    fn export_header_layout_with_one_data_row() {
        for header_row in [2, 5] {
            let config = config(header_row);
            config.check().unwrap();
            let settings = &config.sheets[0];
            let mut book = Workbook::new();
            let mut sheet = start(&mut book, settings, 1).unwrap();
            sheet.write_string(header_row, 0, "PC-DEMO-001").unwrap();
            finish(&mut sheet, settings, 1).unwrap();
            book.push_worksheet(sheet);

            let dir = tempfile::tempdir().unwrap();
            let file = dir.path().join("layout.xlsx");
            book.save(&file).unwrap();
            crate::excel::guard_export(&file, 512).unwrap();
            assert!(crate::excel::guard(&file, 512).is_err());

            let mut reader: Xlsx<_> = open_workbook(&file).unwrap();
            let range = reader.worksheet_range(&settings.name).unwrap();
            assert_eq!(
                range.get_value((0, 0)),
                Some(&Data::String("Project 1".into()))
            );
            assert_eq!(
                range.get_value((header_row - 2, 3)),
                Some(&Data::String("Unit".into()))
            );
            assert_eq!(
                range.get_value((header_row - 1, 0)),
                Some(&Data::String(settings.columns[0].excel_header.clone()))
            );
            assert_eq!(
                range.get_value((header_row, 0)),
                Some(&Data::String("PC-DEMO-001".into()))
            );
            assert_eq!(range.height(), header_row as usize + 1);

            let mut zip = zip::ZipArchive::new(File::open(file).unwrap()).unwrap();
            let mut xml = String::new();
            zip.by_name("xl/worksheets/sheet1.xml")
                .unwrap()
                .read_to_string(&mut xml)
                .unwrap();
            assert!(xml.contains(&format!("<mergeCell ref=\"A1:C{}\"", header_row - 1)));
            assert!(xml.contains(&format!(
                "<autoFilter ref=\"A{header_row}:G{}\"",
                header_row + 1
            )));
            xml.clear();
            zip.by_name("xl/styles.xml")
                .unwrap()
                .read_to_string(&mut xml)
                .unwrap();
            assert!(xml.contains("FF245A81"));
            assert!(xml.contains("FFFFF0CB"));
        }
    }

    #[test]
    fn export_header_layout_rejects_overlapping_or_data_cells() {
        let mut config = config(5);
        config.sheets[0].header.layout[0].column = 3;
        assert!(
            config
                .check()
                .unwrap_err()
                .to_string()
                .contains("Overlapping")
        );
        config.sheets[0].header.layout[0].column = 4;
        config.sheets[0].header.layout[0].row = 5;
        assert!(
            config
                .check()
                .unwrap_err()
                .to_string()
                .contains("above header.row")
        );
    }
}
