//! XMLTV timestamp parsing and formatting.
//!
//! This crate accepts the documented XMLTV date precisions based on an
//! initial substring of `YYYYMMDDhhmmss`, which means exactly 4, 6, 8, 10,
//! 12, or 14 digits, plus an optional `Z`, `±HHMM`, or named timezone suffixes
//! from XMLTV's upstream `Date::Manip` short-abbreviation table such as `BST`,
//! `HKT`, or `GMT+10`.

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
/// optional; when present it must be `Z`, `±HHMM`, or a named timezone from
/// XMLTV's upstream `Date::Manip` short-abbreviation table, such as `BST`,
/// `HKT`, or `GMT+10`.
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
            "timezone suffix `{s}` must be `Z`, `±HHMM`, or a supported XMLTV named timezone abbreviation"
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

// XMLTV delegates named-zone parsing to Date::Manip. Keep a deterministic
// compatibility subset by mirroring Date::Manip's short-abbreviation table
// instead of guessing arbitrary names or depending on host timezone data.
const DATE_MANIP_NAMED_TZ_OFFSETS: &[(&str, i16)] = &[
    ("A", -60),
    ("ACDT", 630),
    ("ACST", 570),
    ("ADDT", -120),
    ("ADT", -180),
    ("AEDT", 660),
    ("AEST", 600),
    ("AHDT", -540),
    ("AHST", -600),
    ("AKDT", -480),
    ("AKST", -540),
    ("APT", -540),
    ("AST", -240),
    ("AT", -120),
    ("AWDT", 540),
    ("AWST", 480),
    ("AWT", -180),
    ("B", -120),
    ("BDST", 120),
    ("BDT", -600),
    ("BST", 60),
    ("BT", 180),
    ("C", -180),
    ("CADT", 630),
    ("CAST", 180),
    ("CAT", 120),
    ("CDT", -300),
    ("CEMT", 180),
    ("CEST", 120),
    ("CET", 60),
    ("CHST", 600),
    ("CLDT", -180),
    ("CMT", 115),
    ("CPT", -300),
    ("CST", -360),
    ("CWT", -300),
    ("D", -240),
    ("E", -300),
    ("EADT", 660),
    ("EAT", 180),
    ("EDT", -240),
    ("EEST", 180),
    ("EET", 120),
    ("EETDST", 180),
    ("EETEDT", 180),
    ("EPT", -240),
    ("EST", -300),
    ("EWT", -240),
    ("F", -360),
    ("FST", 120),
    ("FWT", 60),
    ("G", -420),
    ("GB", 60),
    ("GDT", 660),
    ("GMT", 0),
    ("GMT+1", 60),
    ("GMT+10", 600),
    ("GMT+11", 660),
    ("GMT+12", 720),
    ("GMT+2", 120),
    ("GMT+3", 180),
    ("GMT+4", 240),
    ("GMT+5", 300),
    ("GMT+6", 360),
    ("GMT+7", 420),
    ("GMT+8", 480),
    ("GMT+9", 540),
    ("GMT-1", -60),
    ("GMT-10", -600),
    ("GMT-11", -660),
    ("GMT-12", -720),
    ("GMT-13", -780),
    ("GMT-14", -840),
    ("GMT-2", -120),
    ("GMT-3", -180),
    ("GMT-4", -240),
    ("GMT-5", -300),
    ("GMT-6", -360),
    ("GMT-7", -420),
    ("GMT-8", -480),
    ("GMT-9", -540),
    ("GST", 600),
    ("H", -480),
    ("HDT", -540),
    ("HKST", 540),
    ("HKT", 480),
    ("HKWT", 510),
    ("HPT", -570),
    ("HST", -600),
    ("HWT", -570),
    ("I", -540),
    ("IDDT", 240),
    ("IDLE", 720),
    ("IDLW", -720),
    ("IDT", 180),
    ("IST", 330),
    ("IT", 210),
    ("JDT", 600),
    ("JST", 540),
    ("K", -600),
    ("KDT", 600),
    ("KST", 540),
    ("L", -660),
    ("M", -720),
    ("MDT", -360),
    ("MESZ", 120),
    ("METDST", 120),
    ("MEWT", 60),
    ("MEZ", 60),
    ("MMT", 294),
    ("MPT", -360),
    ("MSD", 240),
    ("MSK", 180),
    ("MST", -420),
    ("MWT", -360),
    ("N", 60),
    ("NDDT", -90),
    ("NDT", -150),
    ("NPT", -600),
    ("NST", -210),
    ("NT", -660),
    ("NWT", -600),
    ("NZDT", 780),
    ("NZMT", 690),
    ("NZST", 720),
    ("NZT", 720),
    ("O", 120),
    ("P", 180),
    ("PDT", -420),
    ("PKST", 360),
    ("PKT", 300),
    ("PPMT", -289),
    ("PPT", -420),
    ("PST", -480),
    ("PWT", -420),
    ("Q", 240),
    ("QMT", -314),
    ("R", 300),
    ("ROK", 540),
    ("S", 360),
    ("SAST", 120),
    ("SAT", -240),
    ("SDMT", -280),
    ("SMT", 136),
    ("SST", -660),
    ("SWT", 60),
    ("T", 420),
    ("TMT", 99),
    ("U", 480),
    ("UT", 0),
    ("UTC", 0),
    ("V", 540),
    ("W", 600),
    ("WAST", 120),
    ("WAT", 60),
    ("WEMT", 120),
    ("WEST", 60),
    ("WET", 0),
    ("WIB", 420),
    ("WIT", 540),
    ("WITA", 480),
    ("WMT", 84),
    ("X", 660),
    ("Y", 720),
    ("YDDT", -420),
    ("YDT", -480),
    ("YPT", -480),
    ("YST", -540),
    ("YWT", -480),
    ("Z", 0),
    ("ZP4", 240),
    ("ZP5", 300),
    ("ZP6", 360),
];

fn parse_named_tz_offset_minutes(s: &str) -> Option<i64> {
    let upper = s.trim().to_ascii_uppercase();
    DATE_MANIP_NAMED_TZ_OFFSETS
        .iter()
        .find_map(|(name, minutes)| (*name == upper).then_some(i64::from(*minutes)))
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

    fn assert_named_zone_matches_offset(zone: &str, minutes: i64) {
        let ts = try_parse_xmltv_timestamp(&format!("20250115120000 {zone}")).unwrap();
        let expected =
            try_parse_xmltv_timestamp(&format!("20250115120000 {}", format_offset(minutes)))
                .unwrap();
        assert_eq!(
            ts, expected,
            "zone `{zone}` should map to {minutes} minutes"
        );
    }

    fn format_offset(minutes: i64) -> String {
        let sign = if minutes < 0 { '-' } else { '+' };
        let abs = minutes.abs();
        format!("{sign}{:02}{:02}", abs / 60, abs % 60)
    }

    #[test]
    fn parse_named_timezones_from_upstream_date_manip_table() {
        for (zone, minutes) in [
            ("A", -60),
            ("N", 60),
            ("HKT", 480),
            ("MESZ", 120),
            ("MMT", 294),
            ("NZDT", 780),
            ("ROK", 540),
            ("GMT+10", 600),
            ("GMT-14", -840),
        ] {
            assert_named_zone_matches_offset(zone, minutes);
        }
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
