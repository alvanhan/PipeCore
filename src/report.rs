use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Detail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<std::collections::BTreeMap<String, Option<String>>>,
}
impl Detail {
    pub fn new(code: &str, message: &str) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            sheet: None,
            row: None,
            column: None,
            field: None,
            expected: None,
            actual: None,
            key: None,
        }
    }
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub input_rows: u64,
    pub validated_rows: u64,
    pub matched_rows: u64,
    pub missing_rows: u64,
    pub inserted_rows: u64,
    pub updated_rows: u64,
    pub deleted_rows: u64,
    pub exported_rows: u64,
    pub error_count: u64,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub status: String,
    pub code: String,
    pub job_id: String,
    pub operation: String,
    pub mode: Option<String>,
    pub driver: Option<String>,
    pub committed: bool,
    pub commit_outcome: String,
    pub summary: Summary,
    pub errors: Vec<Detail>,
    pub error_report_path: Option<String>,
    pub elapsed_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<String>,
    #[serde(skip)]
    pub max_samples: usize,
    #[serde(skip)]
    pub writer: Option<BufWriter<File>>,
}
impl Report {
    pub fn new(operation: &str) -> Self {
        Self {
            status: "SUCCESS".into(),
            code: "SUCCESS".into(),
            job_id: uuid::Uuid::new_v4().to_string(),
            operation: operation.into(),
            mode: None,
            driver: None,
            committed: false,
            commit_outcome: "not_committed".into(),
            summary: Summary::default(),
            errors: vec![],
            error_report_path: None,
            elapsed_ms: 0,
            output_format: None,
            max_samples: 20,
            writer: None,
        }
    }
    pub fn open_errors(&mut self, dir: &Path) -> Result<()> {
        std::fs::create_dir_all(dir)?;
        let p = dir.join(format!("{}.errors.jsonl", self.job_id));
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        self.writer = Some(BufWriter::new(opts.open(&p)?));
        self.error_report_path = Some(p.to_string_lossy().into());
        Ok(())
    }
    pub fn error(&mut self, d: Detail) -> Result<()> {
        if let Some(w) = &mut self.writer {
            serde_json::to_writer(&mut *w, &d)?;
            w.write_all(b"\n")?;
        }
        self.summary.error_count += 1;
        if self.errors.len() < self.max_samples {
            self.errors.push(d);
        }
        Ok(())
    }
    pub fn fail(&mut self, code: &str) {
        self.status = "FAILED".into();
        self.code = code.into();
    }
    pub fn finish(&mut self) -> Result<()> {
        if let Some(mut w) = self.writer.take() {
            w.flush()?;
        }
        if self.summary.error_count == 0
            && let Some(p) = self.error_report_path.take()
        {
            std::fs::remove_file(p)?;
        }
        Ok(())
    }
}
pub fn column(mut i: usize) -> String {
    let mut s = String::new();
    i += 1;
    while i > 0 {
        i -= 1;
        s.insert(0, (b'A' + (i % 26) as u8) as char);
        i /= 26;
    }
    s
}
