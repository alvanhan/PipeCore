mod common;
use calamine::{Data, ExcelDateTime, ExcelDateTimeType};
use pipecore::{config::SheetSelector, excel, report::Report, types::convert};
use rust_xlsxwriter::{Format, Workbook};
fn workbook(headers: &[&str], rows: &[Vec<&str>], name: &str, path: &std::path::Path) {
    let mut b = Workbook::new();
    let s = b.add_worksheet();
    s.set_name(name).unwrap();
    for (i, h) in headers.iter().enumerate() {
        s.write_string(0, i as u16, *h).unwrap();
    }
    for (r, row) in rows.iter().enumerate() {
        for (i, v) in row.iter().enumerate() {
            s.write_string(r as u32 + 1, i as u16, *v).unwrap();
        }
    }
    b.save(path).unwrap();
}
#[test]
fn duplicate_source_keys() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("d.xlsx");
    let mut c = common::config("bulk-update");
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    let headers: Vec<_> = c.fields.iter().map(|f| f.excel.as_str()).collect();
    workbook(
        &headers,
        &[
            vec!["TX1", "2026-01-01 00:00:00", "1.0000", "true"],
            vec!["TX1", "2026-01-01 00:00:00", "2.0000", "false"],
        ],
        "Updates",
        &p,
    );
    let mut r = Report::new("validate");
    excel::validate(&c, &p, &["transaction_id".into()], &mut r).unwrap();
    assert_eq!(r.summary.error_count, 1);
    assert_eq!(r.errors[0].code, "DUPLICATE_SOURCE_KEY");
    assert_eq!(r.errors[0].row, Some(3));
}
#[test]
fn missing_extra_duplicate_headers() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("d.xlsx");
    let mut c = common::config("bulk-update");
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    for headers in [
        vec!["Transaction ID"],
        vec![
            "Transaction ID",
            "Event Time",
            "Balance After",
            "Reconciled",
            "Extra",
        ],
        vec![
            "Transaction ID",
            "Event Time",
            "Balance After",
            "Balance After",
        ],
    ] {
        workbook(&headers, &[], "Updates", &p);
        assert!(excel::validate(&c, &p, &[], &mut Report::new("validate")).is_err());
    }
}
#[test]
fn missing_sheet_and_wrong_header_row() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("d.xlsx");
    let mut c = common::config("bulk-update");
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    common::fixture(&c, &p, 0, 1, false);
    c.excel.as_mut().unwrap().sheet = Some("missing".into());
    assert!(excel::validate(&c, &p, &[], &mut Report::new("validate")).is_err());
    c.excel.as_mut().unwrap().sheet = Some("Updates".into());
    c.excel.as_mut().unwrap().header_row = 2;
    c.excel.as_mut().unwrap().first_data_row = 3;
    assert!(excel::validate(&c, &p, &[], &mut Report::new("validate")).is_err());
}
#[test]
fn merged_headers_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("d.xlsx");
    let mut b = Workbook::new();
    b.add_worksheet()
        .merge_range(0, 0, 0, 1, "Merged", &Format::new())
        .unwrap();
    b.save(&p).unwrap();
    assert!(excel::guard(&p, 512).is_err());
}
#[test]
fn multi_sheet_duplicate_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("d.xlsx");
    let mut c = common::config("bulk-update");
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    let mut b = Workbook::new();
    for name in ["First", "Second"] {
        let s = b.add_worksheet();
        s.set_name(name).unwrap();
        for (i, f) in c.fields.iter().enumerate() {
            s.write_string(0, i as u16, &f.excel).unwrap();
        }
        for (i, v) in ["TX1", "2026-01-01 00:00:00", "1.0000", "true"]
            .iter()
            .enumerate()
        {
            s.write_string(1, i as u16, *v).unwrap();
        }
    }
    b.save(&p).unwrap();
    let e = c.excel.as_mut().unwrap();
    e.sheet = None;
    e.sheets = Some(SheetSelector {
        mode: "named".into(),
        names: vec!["First".into(), "Second".into()],
        same_layout: true,
    });
    let mut r = Report::new("validate");
    excel::validate(&c, &p, &["transaction_id".into()], &mut r).unwrap();
    assert_eq!(r.errors[0].sheet.as_deref(), Some("Second"));
    assert_eq!(r.errors[0].row, Some(2));
}
#[test]
fn report_samples_bounded_full_jsonl() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("d.xlsx");
    let mut c = common::config("bulk-update");
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    let headers: Vec<_> = c.fields.iter().map(|f| f.excel.as_str()).collect();
    workbook(
        &headers,
        &[
            vec!["TX1", "invalid", "bad", "bad"],
            vec!["TX2", "invalid", "bad", "bad"],
        ],
        "Updates",
        &p,
    );
    let mut r = Report::new("validate");
    r.max_samples = 2;
    r.open_errors(dir.path()).unwrap();
    excel::validate(&c, &p, &[], &mut r).unwrap();
    r.finish().unwrap();
    assert_eq!(r.errors.len(), 2);
    assert_eq!(r.summary.error_count, 6);
    let text = std::fs::read_to_string(r.error_report_path.unwrap()).unwrap();
    assert_eq!(text.lines().count(), 6);
}
#[test]
fn sensitive_data_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("d.xlsx");
    let mut c = common::config("bulk-update");
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    c.fields[0].sensitive = true;
    c.fields[0].max_length = Some(2);
    let headers: Vec<_> = c.fields.iter().map(|f| f.excel.as_str()).collect();
    workbook(
        &headers,
        &[vec!["PRIVATE_SECRET", "2026-01-01 00:00:00", "1", "true"]],
        "Updates",
        &p,
    );
    let mut r = Report::new("validate");
    excel::validate(&c, &p, &[], &mut r).unwrap();
    assert!(
        !serde_json::to_string(&r)
            .unwrap()
            .contains("PRIVATE_SECRET")
    );
}
#[test]
fn null_trim_enum_regex_and_ranges() {
    let c = common::config("import");
    let mut f = c.fields[0].clone();
    f.required = false;
    f.trim = true;
    f.empty_as_null = true;
    assert_eq!(convert(&Data::String("  ".into()), &f).unwrap(), None);
    f.choices = vec!["ABC".into()];
    assert!(convert(&Data::String("abc".into()), &f).is_err());
    f.choices.clear();
    f.regex = Some("^[A-Z]+$".into());
    assert!(convert(&Data::String("123".into()), &f).is_err());
    let mut f = c
        .fields
        .iter()
        .find(|f| f.column == "debit_amount")
        .unwrap()
        .clone();
    f.min = Some("1234567890123.1234".into());
    assert!(convert(&Data::String("1234567890123.1233".into()), &f).is_err());
    assert!(convert(&Data::String("1234567890123.1234".into()), &f).is_ok());
}
#[test]
fn date_systems_and_offset() {
    let c = common::config("import");
    let f = c
        .fields
        .iter()
        .find(|f| f.column == "posting_date")
        .unwrap();
    let d = Data::DateTime(ExcelDateTime::new(0.0, ExcelDateTimeType::DateTime, true));
    assert_eq!(convert(&d, f).unwrap(), Some("1904-01-01".into()));
    let d = Data::DateTime(ExcelDateTime::new(1.0, ExcelDateTimeType::DateTime, false));
    assert_eq!(convert(&d, f).unwrap(), Some("1900-01-01".into()));
    let f = c.fields.iter().find(|f| f.column == "settled_at").unwrap();
    assert!(convert(&Data::String("2026-01-02T03:04:05.123456+07:00".into()), f).is_ok());
    assert!(convert(&Data::String("2026-01-02 03:04:05".into()), f).is_err());
}
#[test]
fn zero_rows_valid() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("d.xlsx");
    let mut c = common::config("import");
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    common::fixture(&c, &p, 0, 0, false);
    let mut r = Report::new("validate");
    assert_eq!(excel::validate(&c, &p, &[], &mut r).unwrap().count, 0);
    assert_eq!(r.summary.error_count, 0);
}
