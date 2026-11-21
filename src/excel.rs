use crate::{
    config::Config,
    report::{Detail, Report, column},
    types::convert,
};
use anyhow::{Result, ensure};
use calamine::{Data, Reader, Xlsx};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Read, Write},
    path::Path,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct Row {
    pub sheet: String,
    pub row: u32,
    pub values: Vec<Option<String>>,
}
pub struct Spool {
    pub file: tempfile::NamedTempFile,
    pub count: u64,
}
impl Spool {
    pub fn rows(&self) -> Result<impl Iterator<Item = Result<Row>>> {
        Ok(BufReader::new(File::open(self.file.path())?)
            .lines()
            .map(|s| Ok(serde_json::from_str(&s?)?)))
    }
}
// ZIP budgets cover metadata loaded eagerly by calamine as well as streamed XML.
pub fn guard(path: &Path, memory_mb: u64) -> Result<()> {
    ensure!(
        path.extension()
            .is_some_and(|x| x.eq_ignore_ascii_case("xlsx")),
        "FILE_UNSUPPORTED"
    );
    let mut zip = zip::ZipArchive::new(File::open(path)?)?;
    ensure!(zip.len() <= 4096, "FILE_LIMIT_EXCEEDED");
    let mut total = 0u64;
    let mut sheets = 0;
    for i in 0..zip.len() {
        crate::check_cancelled()?;
        let entry = zip.by_index(i)?;
        let name = entry.name()?;
        ensure!(
            !name.contains("vbaProject") && !name.contains("../") && !name.starts_with('/'),
            "FILE_UNSUPPORTED"
        );
        total = total
            .checked_add(entry.size())
            .ok_or_else(|| anyhow::anyhow!("FILE_LIMIT_EXCEEDED"))?;
        ensure!(total <= 32 * 1024 * 1024 * 1024, "FILE_LIMIT_EXCEEDED");
        if name == "[Content_Types].xml" {
            ensure!(entry.size() <= 1024 * 1024, "FILE_LIMIT_EXCEEDED");
            let mut content = String::new();
            entry.take(1024 * 1024 + 1).read_to_string(&mut content)?;
            ensure!(
                !content.contains("macroEnabled") && !content.contains("vbaProject"),
                "FILE_UNSUPPORTED"
            );
            continue;
        }
        if name == "xl/sharedStrings.xml" || name == "xl/styles.xml" {
            ensure!(
                entry.size() <= memory_mb * 1024 * 1024 / 16,
                "METADATA_MEMORY_LIMIT"
            );
        }
        if name.starts_with("xl/worksheets/") && name.ends_with(".xml") {
            sheets += 1;
            ensure!(sheets <= 256, "SHEET_LIMIT_EXCEEDED");
            let mut xml = quick_xml::Reader::from_reader(BufReader::new(entry));
            let mut buf = Vec::new();
            loop {
                crate::check_cancelled()?;
                match xml.read_event_into(&mut buf)? {
                    quick_xml::events::Event::Empty(e) | quick_xml::events::Event::Start(e) => {
                        ensure!(
                            e.local_name().as_ref() != "mergeCell",
                            "MERGED_CELLS_UNSUPPORTED"
                        );
                    }
                    quick_xml::events::Event::DocType(_) => anyhow::bail!("FILE_UNSUPPORTED"),
                    quick_xml::events::Event::Eof => break,
                    _ => {}
                }
                ensure!(
                    buf.len() <= memory_mb as usize * 1024 * 1024 / 16,
                    "CELL_MEMORY_LIMIT"
                );
                buf.clear();
            }
        }
    }
    ensure!(sheets > 0, "FILE_INVALID");
    Ok(())
}
pub fn validate(c: &Config, path: &Path, keys: &[String], report: &mut Report) -> Result<Spool> {
    guard(path, c.execution.memory_limit_mb)?;
    std::fs::create_dir_all(&c.execution.temp_dir)?;
    let dir = tempfile::tempdir_in(&c.execution.temp_dir)?;
    let dedup = rusqlite::Connection::open(dir.path().join("keys.db"))?;
    dedup.execute_batch("PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF; PRAGMA cache_size=-4096; CREATE TABLE keys(k TEXT PRIMARY KEY) WITHOUT ROWID; BEGIN;")?;
    let file = tempfile::NamedTempFile::new_in(&c.execution.temp_dir)?;
    let mut w = BufWriter::new(file.reopen()?);
    let mut count = 0;
    let mut book: Xlsx<_> = calamine::open_workbook(path)?;
    let is_1904 = book.has_1904_epoch();
    let e = c.excel.as_ref().unwrap();
    let names = if let Some(n) = &e.sheet {
        vec![n.clone()]
    } else {
        e.sheets.as_ref().unwrap().names.clone()
    };
    for name in names {
        ensure!(book.sheet_names().contains(&name), "SHEET_NOT_FOUND");
        let mut reader = book.worksheet_cells_reader(&name)?;
        let mut current = 0u32;
        let mut seen = false;
        let mut header = false;
        let mut cells = vec![Data::Empty; c.fields.len()];
        let mut formulas = vec![false; c.fields.len()];
        let mut extra = false;
        let mut row_bytes = 0usize;
        let mut previous = None;
        let mut flush = |row: u32, cells: &[Data], formulas: &[bool], extra: bool| -> Result<()> {
            if row == e.header_row {
                header = true;
                ensure!(
                    !extra
                        && !formulas.iter().any(|f| *f)
                        && cells
                            .iter()
                            .zip(&c.fields)
                            .all(|(v, f)| matches!(v,Data::String(s) if *s==f.excel)),
                    "HEADER_MISMATCH"
                );
            } else if row >= e.first_data_row
                && (extra
                    || cells.iter().any(|d| !matches!(d, Data::Empty))
                    || formulas.iter().any(|x| *x))
            {
                ensure!(header, "HEADER_MISMATCH");
                report.summary.input_rows += 1;
                let before = report.summary.error_count;
                let mut values = Vec::with_capacity(c.fields.len());
                for (i, (cell, f)) in cells.iter().zip(&c.fields).enumerate() {
                    let result = if formulas[i] {
                        Err(anyhow::anyhow!("FORMULA_REJECTED"))
                    } else if !is_1904
                        && matches!(cell, Data::DateTime(v) if v.as_f64().floor() == 60.0)
                    {
                        Err(anyhow::anyhow!("DATE_INVALID"))
                    } else {
                        convert(cell, f)
                    };
                    match result {
                        Ok(v) => values.push(v),
                        Err(err) => {
                            let mut d = Detail::new(
                                &err.to_string(),
                                "Cell does not satisfy field contract",
                            );
                            d.sheet = Some(name.clone());
                            d.row = Some(row);
                            d.column = Some(column(i));
                            d.field = Some(f.column.clone());
                            d.expected = Some(f.kind.clone());
                            d.actual = Some(if f.sensitive {
                                "[REDACTED]".into()
                            } else {
                                cell.to_string().chars().take(80).collect()
                            });
                            report.error(d)?;
                            values.push(None);
                        }
                    }
                }
                if extra {
                    let mut d = Detail::new("UNKNOWN_COLUMN", "Unexpected column outside template");
                    d.sheet = Some(name.clone());
                    d.row = Some(row);
                    report.error(d)?;
                }
                if report.summary.error_count == before && !keys.is_empty() {
                    let key: Vec<_> = keys
                        .iter()
                        .map(|k| &values[c.fields.iter().position(|f| f.column == *k).unwrap()])
                        .collect();
                    if key.iter().any(|v| v.is_none()) {
                        let mut d = Detail::new("REQUIRED_FIELD", "Source key is null");
                        d.sheet = Some(name.clone());
                        d.row = Some(row);
                        report.error(d)?;
                    } else {
                        let serialized = serde_json::to_string(&key)?;
                        let inserted = dedup
                            .execute("INSERT OR IGNORE INTO keys(k) VALUES (?1)", [serialized])?;
                        if inserted == 0 {
                            let mut d = Detail::new("DUPLICATE_SOURCE_KEY", "Duplicate source key");
                            d.sheet = Some(name.clone());
                            d.row = Some(row);
                            d.column = Some(column(
                                c.fields.iter().position(|f| f.column == keys[0]).unwrap(),
                            ));
                            report.error(d)?;
                        }
                    }
                }
                if report.summary.error_count == before {
                    let r = Row {
                        sheet: name.clone(),
                        row,
                        values,
                    };
                    serde_json::to_writer(&mut w, &r)?;
                    w.write_all(b"\n")?;
                    count += 1;
                    report.summary.validated_rows += 1;
                }
            }
            Ok(())
        };
        while let Some(cell) = reader.next_cell_with_formula()? {
            crate::check_cancelled()?;
            ensure!(
                cell.pos.0 < 1048576 && cell.pos.1 < 16384 && previous.is_none_or(|p| p < cell.pos),
                "MALFORMED_ROW"
            );
            previous = Some(cell.pos);
            let row = cell.pos.0 + 1;
            if seen && row != current {
                flush(current, &cells, &formulas, extra)?;
                cells.fill(Data::Empty);
                formulas.fill(false);
                extra = false;
                row_bytes = 0;
            }
            row_bytes += match &cell.value {
                calamine::DataRef::String(s) => s.len(),
                calamine::DataRef::SharedString(s) => s.len(),
                _ => 16,
            };
            ensure!(
                row_bytes <= c.execution.memory_limit_mb as usize * 1024 * 1024 / 8,
                "ROW_MEMORY_LIMIT"
            );
            current = row;
            seen = true;
            let col = cell.pos.1 as usize;
            if col < cells.len() {
                cells[col] = cell.value.into();
                formulas[col] = cell.formula.is_some();
            } else {
                extra = true;
            }
        }
        if seen {
            flush(current, &cells, &formulas, extra)?;
        }
        ensure!(header, "HEADER_MISMATCH");
    }
    w.flush()?;
    Ok(Spool { file, count })
}
