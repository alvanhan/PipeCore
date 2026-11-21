use pipecore::config::Config;
use rust_xlsxwriter::Workbook;
use std::path::Path;
pub fn config(name: &str) -> Config {
    Config::read(Path::new(&format!("examples/{name}-finance.json"))).unwrap()
}
pub fn fixture(c: &Config, path: &Path, start: u64, count: u64, invalid: bool) {
    let mut book = Workbook::new();
    let names = c
        .excel
        .as_ref()
        .unwrap()
        .sheet
        .clone()
        .map(|s| vec![s])
        .unwrap_or_else(|| {
            c.excel
                .as_ref()
                .unwrap()
                .sheets
                .as_ref()
                .unwrap()
                .names
                .clone()
        });
    for (page, name) in names.iter().enumerate() {
        let sheet = book.add_worksheet_with_constant_memory().unwrap();
        sheet.set_name(name).unwrap();
        for (i, f) in c.fields.iter().enumerate() {
            sheet.write_string(0, i as u16, &f.excel).unwrap();
        }
        let first = page as u64 * 1048575;
        for n in first..count.min(first + 1048575) {
            for (i, f) in c.fields.iter().enumerate() {
                let value = match f.column.as_str() {
                    "transaction_id" => format!("TX{:012}", start + n),
                    "account_id" => "ACC001".into(),
                    "customer_id" => "CUST001".into(),
                    "reference_no" => "".into(),
                    "posting_date" => "2026-01-02".into(),
                    "event_time" | "created_at" => "2026-01-02 10:11:12.123456".into(),
                    "settled_at" => "2026-01-02T10:11:12.123456+07:00".into(),
                    "time_of_day" => "10:11:12.123456".into(),
                    "currency_code" => "IDR".into(),
                    "debit_amount" => {
                        if invalid && n == count - 1 {
                            "1.12345".into()
                        } else {
                            "1234567890123.1234".into()
                        }
                    }
                    "credit_amount" | "tax_amount" => "0.0000".into(),
                    "balance_after" => "234567890123456789.1234".into(),
                    "fx_rate" => "1.12345678".into(),
                    "quantity" => "9007199254740993".into(),
                    "units" => "42".into(),
                    "is_reconciled" => "true".into(),
                    "branch_code" => "JKT".into(),
                    "description" => "=literal + unicode 日本 😀".into(),
                    "source_system" => "TEST".into(),
                    "batch_no" => "-1".into(),
                    "risk_score" => "0.25".into(),
                    "external_uuid" => "f47ac10b-58cc-4372-a567-0e02b2c3d479".into(),
                    "smallint_code" => "32767".into(),
                    _ => panic!("unknown field"),
                };
                sheet
                    .write_string((n - first) as u32 + 1, i as u16, value)
                    .unwrap();
            }
        }
    }
    book.save(path).unwrap();
}
