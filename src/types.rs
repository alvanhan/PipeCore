use crate::config::Field;
use anyhow::{Result, bail, ensure};
use calamine::{Data, DataType};
use chrono::{DateTime, NaiveDate, NaiveDateTime, NaiveTime, Timelike};

// Decimal text is never routed through IEEE754, including DECIMAL(38,s).
pub fn decimal(s: &str, p: u32, scale: u32) -> Result<String> {
    let body = s
        .strip_prefix('-')
        .or_else(|| s.strip_prefix('+'))
        .unwrap_or(s);
    let parts: Vec<_> = body.split('.').collect();
    ensure!(
        parts.len() <= 2
            && !parts[0].is_empty()
            && parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit())),
        "INVALID_DECIMAL"
    );
    let frac = parts.get(1).copied().unwrap_or("");
    ensure!(
        frac.len() <= scale as usize
            && parts[0].trim_start_matches('0').len() <= (p - scale) as usize,
        "INVALID_DECIMAL"
    );
    let int = parts[0].trim_start_matches('0');
    let int = if int.is_empty() { "0" } else { int };
    let negative = s.starts_with('-') && (int != "0" || frac.bytes().any(|b| b != b'0'));
    Ok(format!(
        "{}{}{}",
        if negative { "-" } else { "" },
        int,
        if frac.is_empty() {
            String::new()
        } else {
            format!(".{frac}")
        }
    ))
}
fn numeric_cmp(a: &str, b: &str) -> Result<std::cmp::Ordering> {
    let a = decimal(a, 38, 0)
        .or_else(|_| decimal(a, 38, a.split('.').nth(1).map_or(0, |x| x.len() as u32)))?;
    let b = decimal(b, 38, 0)
        .or_else(|_| decimal(b, 38, b.split('.').nth(1).map_or(0, |x| x.len() as u32)))?;
    let neg_a = a.starts_with('-');
    let neg_b = b.starts_with('-');
    if neg_a != neg_b {
        return Ok(if neg_a {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Greater
        });
    }
    let a = a.trim_start_matches('-');
    let b = b.trim_start_matches('-');
    let (ai, af) = a.split_once('.').unwrap_or((a, ""));
    let (bi, bf) = b.split_once('.').unwrap_or((b, ""));
    let n = af.len().max(bf.len());
    let af = format!("{af:0<width$}", width = n);
    let bf = format!("{bf:0<width$}", width = n);
    let cmp = ai.len().cmp(&bi.len()).then(ai.cmp(bi)).then(af.cmp(&bf));
    Ok(if neg_a { cmp.reverse() } else { cmp })
}
pub fn convert(cell: &Data, f: &Field) -> Result<Option<String>> {
    if matches!(cell, Data::Empty) {
        ensure!(!f.required, "REQUIRED_FIELD");
        return Ok(None);
    }
    ensure!(!matches!(cell, Data::Error(_)), "TYPE_MISMATCH");
    if f.excel_storage.as_deref() == Some("text") {
        ensure!(matches!(cell, Data::String(_)), "TEXT_STORAGE_REQUIRED");
    }
    let mut s = cell.to_string();
    if f.trim {
        s = s.trim().into();
    }
    if (f.empty_as_null && s.is_empty()) || f.null_tokens.contains(&s) {
        ensure!(!f.required, "REQUIRED_FIELD");
        return Ok(None);
    }
    ensure!(!s.contains('\0'), "TYPE_MISMATCH");
    if f.kind != "string" {
        ensure!(!s.is_empty() && s.trim() == s, "TYPE_MISMATCH");
    }
    let value = match f.kind.as_str() {
        "string" => {
            ensure!(
                f.max_length.is_none_or(|n| s.chars().count() <= n),
                "LENGTH_EXCEEDED"
            );
            s.clone()
        }
        "decimal" => {
            ensure!(
                f.precision.unwrap() <= 15 || matches!(cell, Data::String(_)),
                "TEXT_STORAGE_REQUIRED"
            );
            decimal(&s, f.precision.unwrap(), f.scale.unwrap())?
        }
        "int16" => s
            .parse::<i16>()
            .map_err(|_| anyhow::anyhow!("INTEGER_RANGE"))?
            .to_string(),
        "int32" => s
            .parse::<i32>()
            .map_err(|_| anyhow::anyhow!("INTEGER_RANGE"))?
            .to_string(),
        "int64" => {
            if !matches!(cell, Data::String(_)) {
                ensure!(
                    s.trim_start_matches('-').len() <= 15,
                    "TEXT_STORAGE_REQUIRED"
                );
            }
            s.parse::<i64>()
                .map_err(|_| anyhow::anyhow!("INTEGER_RANGE"))?
                .to_string()
        }
        "float64" => {
            let v = s
                .parse::<f64>()
                .map_err(|_| anyhow::anyhow!("TYPE_MISMATCH"))?;
            ensure!(v.is_finite(), "NON_FINITE_NUMBER");
            v.to_string()
        }
        "boolean" => match s.as_str() {
            "true" | "1" => "1".into(),
            "false" | "0" => "0".into(),
            _ => bail!("TYPE_MISMATCH"),
        },
        "uuid" => uuid::Uuid::parse_str(&s)
            .map_err(|_| anyhow::anyhow!("INVALID_UUID"))?
            .to_string(),
        "date" | "time" | "datetime" => {
            let fmt = f.date_format.as_deref();
            match f.kind.as_str() {
                "date" => {
                    let d = if matches!(cell, Data::DateTime(_)) {
                        cell.as_date()
                            .ok_or_else(|| anyhow::anyhow!("DATE_INVALID"))?
                    } else {
                        NaiveDate::parse_from_str(&s, fmt.unwrap_or("%Y-%m-%d"))
                            .map_err(|_| anyhow::anyhow!("DATE_INVALID"))?
                    };
                    d.to_string()
                }
                "time" => {
                    let t = if matches!(cell, Data::DateTime(_)) {
                        cell.as_time()
                            .ok_or_else(|| anyhow::anyhow!("DATE_INVALID"))?
                    } else {
                        NaiveTime::parse_from_str(&s, fmt.unwrap_or("%H:%M:%S%.f"))
                            .map_err(|_| anyhow::anyhow!("DATE_INVALID"))?
                    };
                    ensure!(t.nanosecond() < 1_000_000_000, "DATE_INVALID");
                    t.format("%H:%M:%S%.f").to_string()
                }
                _ => {
                    let d = if matches!(cell, Data::DateTime(_)) {
                        cell.as_datetime()
                            .ok_or_else(|| anyhow::anyhow!("DATE_INVALID"))?
                    } else {
                        NaiveDateTime::parse_from_str(&s, fmt.unwrap_or("%Y-%m-%d %H:%M:%S%.f"))
                            .map_err(|_| anyhow::anyhow!("DATE_INVALID"))?
                    };
                    ensure!(d.nanosecond() < 1_000_000_000, "DATE_INVALID");
                    d.format("%Y-%m-%d %H:%M:%S%.f").to_string()
                }
            }
        }
        "datetime_offset" => {
            let d =
                DateTime::parse_from_rfc3339(&s).map_err(|_| anyhow::anyhow!("DATE_INVALID"))?;
            ensure!(d.nanosecond() < 1_000_000_000, "DATE_INVALID");
            d.to_rfc3339()
        }
        _ => bail!("TYPE_MISMATCH"),
    };
    ensure!(
        f.choices.is_empty() || f.choices.contains(&value),
        "ENUM_MISMATCH"
    );
    if let Some(r) = &f.regex {
        ensure!(regex::Regex::new(r)?.is_match(&value), "REGEX_MISMATCH");
    }
    for (bound, is_min) in [(&f.min, true), (&f.max, false)] {
        if let Some(b) = bound {
            let cmp = if f.kind == "float64" {
                value
                    .parse::<f64>()?
                    .partial_cmp(&b.parse::<f64>()?)
                    .ok_or_else(|| anyhow::anyhow!("INVALID_RANGE"))?
            } else {
                numeric_cmp(&value, b)?
            };
            ensure!(
                if is_min { !cmp.is_lt() } else { !cmp.is_gt() },
                "OUT_OF_RANGE"
            );
        }
    }
    Ok(Some(value))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_decimals() {
        assert_eq!(
            decimal("1234567890123.1234", 20, 4).unwrap(),
            "1234567890123.1234"
        );
        assert!(decimal("1.12345", 20, 4).is_err());
        assert!(decimal("10000000000000000", 20, 4).is_err());
        assert!(decimal("1e3", 20, 4).is_err());
        assert!(decimal("99999999999999999999999999999999999999", 38, 0).is_ok());
        assert_eq!(decimal("-0.000", 20, 4).unwrap(), "0.000");
    }
    #[test]
    fn exact_ranges() {
        assert!(
            numeric_cmp("123456789012345678.1", "123456789012345678.2")
                .unwrap()
                .is_lt()
        );
        assert!(numeric_cmp("-2", "-1").unwrap().is_lt());
    }
}
