use std::process::Command;
#[test]
fn json_stdout_and_exit_codes() {
    let binary = env!("CARGO_BIN_EXE_pipecore");
    for (args, code) in [
        (vec!["version"], 0),
        (vec!["--bad-secret-value"], 2),
        (
            vec![
                "validate",
                "--schema",
                "does-not-exist.json",
                "--input",
                "missing.xlsx",
            ],
            2,
        ),
    ] {
        let out = Command::new(binary).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(code));
        let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert!(v["status"].is_string());
        assert!(!String::from_utf8_lossy(&out.stdout).contains("bad-secret-value"));
    }
}
#[test]
fn report_open_failure_prevents_job() {
    let out = Command::new(env!("CARGO_BIN_EXE_pipecore"))
        .args([
            "import",
            "--schema",
            "examples/import-finance.json",
            "--input",
            "missing.xlsx",
            "--report",
            "nonexistent-report-directory/result.json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(7));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["committed"], false);
}
