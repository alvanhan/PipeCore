mod common;
use calamine::Data;
use pipecore::{config::Config, excel, report::Report, types::convert};
#[test]
fn example_contracts() {
    for name in ["import", "export", "bulk-update", "upsert"] {
        common::config(name).check().unwrap();
    }
}
#[test]
fn config_fail_closed() {
    let c = common::config("import");
    let mut v = serde_json::to_value(c).unwrap();
    v["extra"] = true.into();
    assert!(serde_json::from_value::<Config>(v).is_err());
    let mut c = common::config("import");
    c.schema_version = "2".into();
    assert!(c.check().is_err());
    c.schema_version = "1.0".into();
    c.execution.atomic = false;
    assert!(c.check().is_err());
}
#[test]
fn field_conversions() {
    let c = common::config("import");
    let find = |n: &str| c.fields.iter().find(|f| f.column == n).unwrap();
    assert!(convert(&Data::String("32768".into()), find("smallint_code")).is_err());
    assert!(convert(&Data::String("2147483648".into()), find("units")).is_err());
    assert!(
        convert(
            &Data::String("9223372036854775808".into()),
            find("quantity")
        )
        .is_err()
    );
    assert!(convert(&Data::Float(123.0), find("debit_amount")).is_err());
    assert!(convert(&Data::String("NaN".into()), find("risk_score")).is_err());
    assert!(convert(&Data::String("2026-02-29".into()), find("posting_date")).is_err());
    assert!(convert(&Data::String("yes".into()), find("is_reconciled")).is_err());
    assert!(convert(&Data::String("invalid".into()), find("external_uuid")).is_err());
    assert!(convert(&Data::Empty, find("transaction_id")).is_err());
    assert_eq!(convert(&Data::Empty, find("reference_no")).unwrap(), None);
    assert_eq!(
        convert(&Data::String("".into()), find("reference_no")).unwrap(),
        Some("".into())
    );
    assert_eq!(
        convert(&Data::String("0".into()), find("units")).unwrap(),
        Some("0".into())
    );
}
#[test]
fn validation_provenance() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.xlsx");
    let mut c = common::config("import");
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    common::fixture(&c, &path, 0, 4000, true);
    let mut r = Report::new("validate");
    let spool = excel::validate(&c, &path, &["transaction_id".into()], &mut r).unwrap();
    assert_eq!(spool.count, 3999);
    assert_eq!(r.summary.error_count, 1);
    assert_eq!(r.errors[0].row, Some(4001));
    assert_eq!(r.errors[0].column.as_deref(), Some("J"));
    assert_eq!(r.errors[0].code, "INVALID_DECIMAL");
}
#[test]
fn header_formula_and_duplicate() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("data.xlsx");
    let mut c = common::config("bulk-update");
    c.execution.temp_dir = dir.path().to_string_lossy().into();
    common::fixture(&c, &path, 0, 2, false);
    c.fields.swap(0, 1);
    assert!(excel::validate(&c, &path, &[], &mut Report::new("validate")).is_err());
    c.fields.swap(0, 1);
    let mut book = rust_xlsxwriter::Workbook::new();
    let s = book.add_worksheet();
    s.set_name("Updates").unwrap();
    for (i, f) in c.fields.iter().enumerate() {
        s.write_string(0, i as u16, &f.excel).unwrap();
    }
    s.write_string(1, 0, "TX1").unwrap();
    s.write_string(1, 1, "2026-01-01 00:00:00").unwrap();
    s.write_formula(1, 2, "=1+1").unwrap();
    s.write_boolean(1, 3, true).unwrap();
    book.save(&path).unwrap();
    let mut r = Report::new("validate");
    excel::validate(&c, &path, &[], &mut r).unwrap();
    assert_eq!(r.errors[0].code, "FORMULA_REJECTED");
}
#[test]
fn corrupted_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.xlsx");
    std::fs::write(&path, b"not a zip").unwrap();
    assert!(excel::guard(&path, 512).is_err());
}
