//! XMLTV timestamp parsing and formatting.
//!
//! This crate accepts the documented XMLTV date precisions based on an
//! initial substring of `YYYYMMDDhhmmss`, which means exactly 4, 6, 8, 10,
//! 12, or 14 digits, plus an optional `Z`, `±HHMM`, or supported named
//! timezone suffix such as `BST`.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};

use crate::error::XmltvError;

/// Parse an XMLTV timestamp string into a UTC Unix timestamp (seconds).
///
/// This convenience helper returns `None` for invalid input. Use
/// [`try_parse_xmltv_timestamp`] when you need a precise error.
pub fn parse_xmltv_timestamp(s: &str) -> Option<i64> {
    try_parse_xmltv_timestamp(s).ok()
}

/// Parse an XMLTV timestamp string into a UTC Unix timestamp (seconds).
///
/// Accepted numeric precisions are `YYYY`, `YYYYMM`, `YYYYMMDD`,
/// `YYYYMMDDhh`, `YYYYMMDDhhmm`, and `YYYYMMDDhhmmss`. The timezone suffix is
/// optional; when present it must be `Z`, `±HHMM`, or a supported named
/// timezone abbreviation such as `BST`.
pub fn try_parse_xmltv_timestamp(s: &str) -> Result<i64, XmltvError> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Err(XmltvError::Timestamp("empty timestamp".into()));
    }

    let (numeric, tz_offset) = split_timestamp_parts(trimmed)?;
    let dt = parse_datetime(numeric)?;
    let utc = dt.checked_sub_signed(tz_offset).ok_or_else(|| {
        XmltvError::Timestamp(format!("timestamp `{trimmed}` overflows supported range"))
    })?;

    Ok(utc.and_utc().timestamp())
}

/// Format a UTC Unix timestamp (seconds) into XMLTV format.
///
/// Output is always normalized to `"YYYYMMDDHHmmss +0000"`.
pub fn format_xmltv_timestamp(ts: i64) -> Result<String, XmltvError> {
    let dt = chrono::DateTime::from_timestamp(ts, 0).ok_or_else(|| {
        XmltvError::Timestamp(format!(
            "timestamp `{ts}` is out of range for XMLTV formatting"
        ))
    })?;
    let dt = dt.naive_utc();

    Ok(format!(
        "{:04}{:02}{:02}{:02}{:02}{:02} +0000",
        dt.date().year(),
        dt.date().month(),
        dt.date().day(),
        dt.time().hour(),
        dt.time().minute(),
        dt.time().second(),
    ))
}

/// Validate a raw XMLTV timestamp string without normalizing it.
pub(crate) fn validate_xmltv_timestamp(s: &str) -> Result<(), XmltvError> {
    try_parse_xmltv_timestamp(s).map(|_| ())
}

/// Split timestamp into `(numeric_part, timezone_delta)`.
fn split_timestamp_parts(s: &str) -> Result<(&str, TimeDelta), XmltvError> {
    let numeric_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let numeric = &s[..numeric_end];
    let remainder = s[numeric_end..].trim();

    if numeric.is_empty() {
        return Err(XmltvError::Timestamp(format!(
            "timestamp `{s}` is missing its numeric date portion"
        )));
    }

    let delta = if remainder.is_empty() {
        TimeDelta::zero()
    } else {
        parse_tz_offset(remainder)?
    };

    Ok((numeric, delta))
}

/// Parse timezone offset like `+0530`, `-0800`, `Z`, or `BST`.
fn parse_tz_offset(s: &str) -> Result<TimeDelta, XmltvError> {
    if s.eq_ignore_ascii_case("z") {
        return Ok(TimeDelta::zero());
    }

    if let Some(minutes) = parse_named_tz_offset_minutes(s) {
        return Ok(TimeDelta::minutes(minutes));
    }

    let bytes = s.as_bytes();
    if bytes.len() != 5
        || !matches!(bytes[0], b'+' | b'-')
        || !bytes[1..].iter().all(u8::is_ascii_digit)
    {
        return Err(XmltvError::Timestamp(format!(
            "timezone suffix `{s}` must be `Z`, `±HHMM`, or a supported named timezone abbreviation"
        )));
    }

    let hours: i64 = s[1..3].parse().map_err(|_| {
        XmltvError::Timestamp(format!(
            "timezone suffix `{s}` has an invalid hour component"
        ))
    })?;
    let minutes: i64 = s[3..5].parse().map_err(|_| {
        XmltvError::Timestamp(format!(
            "timezone suffix `{s}` has an invalid minute component"
        ))
    })?;

    if hours > 23 || minutes > 59 {
        return Err(XmltvError::Timestamp(format!(
            "timezone suffix `{s}` is outside the supported `±HHMM` range"
        )));
    }

    let sign: i64 = if bytes[0] == b'-' { -1 } else { 1 };
    Ok(TimeDelta::minutes(sign * (hours * 60 + minutes)))
}

fn parse_named_tz_offset_minutes(s: &str) -> Option<i64> {
    // XMLTV documents may use short named timezone abbreviations such as
    // `BST`; keep the mapping explicit so parsing stays deterministic.
    let upper = s.trim().to_ascii_uppercase();
    let minutes = match upper.as_str() {
        "UTC" | "GMT" | "WET" => 0,
        "BST" | "CET" | "WEST" => 60,
        "CEST" | "EET" => 120,
        "EEST" => 180,
        "AST" => -240,
        "ADT" => -180,
        "EST" => -300,
        "EDT" => -240,
        "CST" => -360,
        "CDT" => -300,
        "MST" => -420,
        "MDT" => -360,
        "PST" => -480,
        "PDT" => -420,
        "AKST" => -540,
        "AKDT" => -480,
        "HST" => -600,
        "JST" | "KST" => 540,
        "AEST" => 600,
        "AEDT" => 660,
        "ACST" => 570,
        "ACDT" => 630,
        "AWST" => 480,
        "NZST" => 720,
        "NZDT" => 780,
        _ => return None,
    };

    Some(minutes)
}

/// Parse the numeric portion of an XMLTV timestamp into a `NaiveDateTime`.
fn parse_datetime(s: &str) -> Result<NaiveDateTime, XmltvError> {
    match s.len() {
        4 | 6 | 8 | 10 | 12 | 14 => {}
        _ => {
            return Err(XmltvError::Timestamp(format!(
                "timestamp `{s}` must use one of the supported XMLTV precisions: YYYY, YYYYMM, YYYYMMDD, YYYYMMDDhh, YYYYMMDDhhmm, or YYYYMMDDhhmmss"
            )));
        }
    }

    let year: i32 = s[..4].parse().map_err(|_| {
        XmltvError::Timestamp(format!("timestamp `{s}` has an invalid year component"))
    })?;
    let month: u32 = if s.len() >= 6 {
        s[4..6].parse().map_err(|_| {
            XmltvError::Timestamp(format!("timestamp `{s}` has an invalid month component"))
        })?
    } else {
        1
    };
    let day: u32 = if s.len() >= 8 {
        s[6..8].parse().map_err(|_| {
            XmltvError::Timestamp(format!("timestamp `{s}` has an invalid day component"))
        })?
    } else {
        1
    };
    let hour: u32 = if s.len() >= 10 {
        s[8..10].parse().map_err(|_| {
            XmltvError::Timestamp(format!("timestamp `{s}` has an invalid hour component"))
        })?
    } else {
        0
    };
    let minute: u32 = if s.len() >= 12 {
        s[10..12].parse().map_err(|_| {
            XmltvError::Timestamp(format!("timestamp `{s}` has an invalid minute component"))
        })?
    } else {
        0
    };
    let second: u32 = if s.len() >= 14 {
        s[12..14].parse().map_err(|_| {
            XmltvError::Timestamp(format!("timestamp `{s}` has an invalid second component"))
        })?
    } else {
        0
    };

    let date = NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| {
        XmltvError::Timestamp(format!(
            "timestamp `{s}` contains an out-of-range calendar date"
        ))
    })?;
    let time = NaiveTime::from_hms_opt(hour, minute, second).ok_or_else(|| {
        XmltvError::Timestamp(format!(
            "timestamp `{s}` contains an out-of-range clock time"
        ))
    })?;

    Ok(NaiveDateTime::new(date, time))
}

use chrono::Datelike as _;
use chrono::Timelike as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_utc_timestamp() {
        let ts = try_parse_xmltv_timestamp("20250115120000 +0000").unwrap();
        assert_eq!(ts, 1_736_942_400);
    }

    #[test]
    fn parse_timestamp_with_positive_offset() {
        let ts = try_parse_xmltv_timestamp("20250115120000 +0530").unwrap();
        let expected = try_parse_xmltv_timestamp("20250115063000 +0000").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_timestamp_with_negative_offset() {
        let ts = try_parse_xmltv_timestamp("20250115120000 -0800").unwrap();
        let expected = try_parse_xmltv_timestamp("20250115200000 +0000").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_no_timezone() {
        let ts = try_parse_xmltv_timestamp("20250115120000").unwrap();
        assert_eq!(ts, 1_736_942_400);
    }

    #[test]
    fn parse_date_only() {
        let ts = try_parse_xmltv_timestamp("20250115").unwrap();
        let expected = try_parse_xmltv_timestamp("20250115000000 +0000").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_year_month_only() {
        let ts = try_parse_xmltv_timestamp("202501").unwrap();
        let expected = try_parse_xmltv_timestamp("20250101000000 +0000").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_year_only() {
        let ts = try_parse_xmltv_timestamp("2025").unwrap();
        let expected = try_parse_xmltv_timestamp("20250101000000 +0000").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_hour_precision() {
        let ts = try_parse_xmltv_timestamp("2025011512").unwrap();
        let expected = try_parse_xmltv_timestamp("20250115120000 +0000").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_minute_precision() {
        let ts = try_parse_xmltv_timestamp("202501151230").unwrap();
        let expected = try_parse_xmltv_timestamp("20250115123000 +0000").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_z_timezone() {
        let ts = try_parse_xmltv_timestamp("20250115120000Z").unwrap();
        let expected = try_parse_xmltv_timestamp("20250115120000 +0000").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_named_timezone_bst() {
        let ts = try_parse_xmltv_timestamp("200007281733 BST").unwrap();
        let expected = try_parse_xmltv_timestamp("200007281733 +0100").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_named_timezone_case_insensitively() {
        let ts = try_parse_xmltv_timestamp("20250115120000 cest").unwrap();
        let expected = try_parse_xmltv_timestamp("20250115120000 +0200").unwrap();
        assert_eq!(ts, expected);
    }

    #[test]
    fn parse_empty_returns_none() {
        assert!(parse_xmltv_timestamp("").is_none());
        assert!(parse_xmltv_timestamp("  ").is_none());
    }

    #[test]
    fn parse_invalid_returns_none() {
        assert!(parse_xmltv_timestamp("not-a-timestamp").is_none());
        assert!(parse_xmltv_timestamp("20251315120000 +0000").is_none());
    }

    #[test]
    fn parse_rejects_trailing_junk() {
        assert!(parse_xmltv_timestamp("20250115120000 +0000junk").is_none());
        assert!(parse_xmltv_timestamp("20250115120000Z trailing").is_none());
    }

    #[test]
    fn parse_rejects_unsupported_precisions() {
        assert!(parse_xmltv_timestamp("202501151").is_none());
        assert!(parse_xmltv_timestamp("2025011512000").is_none());
        assert!(parse_xmltv_timestamp("202501151200001").is_none());
    }

    #[test]
    fn parse_rejects_malformed_suffixes() {
        assert!(parse_xmltv_timestamp("20250115120000 +00").is_none());
        assert!(parse_xmltv_timestamp("20250115120000 +0A00").is_none());
        assert!(parse_xmltv_timestamp("20250115120000 XYZ").is_none());
        assert!(parse_xmltv_timestamp("20250115120000 +2460").is_none());
    }

    #[test]
    fn format_roundtrip() {
        let original = "20250115120000 +0000";
        let ts = try_parse_xmltv_timestamp(original).unwrap();
        let formatted = format_xmltv_timestamp(ts).unwrap();
        assert_eq!(formatted, original);
    }

    #[test]
    fn format_epoch() {
        let formatted = format_xmltv_timestamp(0).unwrap();
        assert_eq!(formatted, "19700101000000 +0000");
    }

    #[test]
    fn format_rejects_out_of_range_timestamp() {
        let err = format_xmltv_timestamp(i64::MAX).unwrap_err();
        assert!(matches!(err, XmltvError::Timestamp(_)));
    }
}
