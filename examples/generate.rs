#[path = "../tests/common/mod.rs"]
mod common;
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let n: u64 = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(4000);
    let path = args
        .get(1)
        .map(String::as_str)
        .unwrap_or("output/finance.xlsx");
    let name = args.get(3).map(String::as_str).unwrap_or("import");
    let mut c = common::config(name);
    if n > 1048575 {
        let e = c.excel.as_mut().unwrap();
        let base = e.sheet.take().unwrap();
        let names = (0..n.div_ceil(1048575))
            .map(|i| {
                if i == 0 {
                    base.clone()
                } else {
                    format!("{base}_{}", i + 1)
                }
            })
            .collect();
        e.sheets = Some(pipecore::config::SheetSelector {
            mode: "named".into(),
            names,
            same_layout: true,
        });
    }
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    common::fixture(&c, std::path::Path::new(path), 0, n, false);
    std::fs::write(
        format!("{path}.schema.json"),
        serde_json::to_string_pretty(&c).unwrap(),
    )
    .unwrap();
}
