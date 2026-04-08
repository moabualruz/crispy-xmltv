//! Streaming XMLTV parser using `quick_xml` event-based pull parsing.
//!
//! Handles 100 MB+ files efficiently by processing `<channel>` and
//! `<programme>` elements as streaming events without buffering the
//! entire DOM.

use std::io::BufRead;

use crispy_iptv_types::epg::{
    EpgAudio, EpgChannel, EpgCredits, EpgEpisodeNumber, EpgIcon, EpgImage, EpgLength,
    EpgLengthUnit, EpgPerson, EpgPreviouslyShown, EpgProgramme, EpgRating, EpgReview,
    EpgStringWithLang, EpgSubtitleType, EpgSubtitles, EpgUrl, EpgVideo,
};
use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};
use smallvec::SmallVec;

use crate::error::XmltvError;
use crate::timestamp::{try_parse_xmltv_timestamp, validate_xmltv_timestamp};
use crate::types::XmltvDocument;

/// Parse XMLTV content from a string.
pub fn parse(content: &str) -> Result<XmltvDocument, XmltvError> {
    let cursor = std::io::Cursor::new(content.as_bytes());
    parse_reader(std::io::BufReader::new(cursor))
}

/// Parse XMLTV content from a byte slice, auto-detecting and decompressing
/// gzip or XZ compression.
///
/// Equivalent to calling `decompress_auto` followed by `parse`.
pub fn parse_compressed(data: &[u8]) -> Result<XmltvDocument, XmltvError> {
    let decompressed = crate::compression::decompress_auto(data)?;
    let content = std::str::from_utf8(&decompressed)
        .map_err(|e| XmltvError::Xml(format!("invalid UTF-8 after decompression: {e}")))?;
    parse(content)
}

/// Parse XMLTV content from a buffered reader (streaming).
///
/// Accepts any `impl BufRead` so the caller controls I/O buffering.
pub fn parse_reader(reader: impl BufRead) -> Result<XmltvDocument, XmltvError> {
    let mut xml_reader = Reader::from_reader(reader);
    xml_reader.config_mut().trim_text(true);
    parse_events(&mut xml_reader)
}

/// Extract channel ID to display name mapping from XMLTV content.
///
/// Utility for quick channel name lookups without parsing full programmes.
pub fn extract_channel_names(
    content: &str,
) -> Result<std::collections::HashMap<String, String>, XmltvError> {
    let doc = parse(content)?;
    let mut result = std::collections::HashMap::new();
    for ch in &doc.channels {
        if let Some(name) = ch.display_name.first()
            && !name.value.is_empty()
        {
            result.entry(ch.id.clone()).or_insert(name.value.clone());
        }
    }
    Ok(result)
}

// ── Core parse loop ─────────────────────────────────────────────

fn parse_events<R: BufRead>(reader: &mut Reader<R>) -> Result<XmltvDocument, XmltvError> {
    let mut doc = XmltvDocument::default();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                // Extract data from the event before calling sub-parsers.
                match e.name().as_ref() {
                    b"channel" => {
                        let id = get_attr(e, b"id").unwrap_or_default();
                        let mut channel = parse_channel_body(reader)?;
                        channel.id = id;
                        doc.channels.push(channel);
                    }
                    b"programme" => {
                        let prog = parse_programme_start(e)?;
                        let prog = parse_programme_body(reader, prog)?;
                        doc.programmes.push(prog);
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(doc)
}

// ── Channel parsing ─────────────────────────────────────────────

fn parse_channel_body<R: BufRead>(reader: &mut Reader<R>) -> Result<EpgChannel, XmltvError> {
    let mut channel = EpgChannel::default();
    let mut depth: u32 = 1;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"display-name" => {
                    let lang = get_attr(e, b"lang");
                    let text = read_text_content(reader)?;
                    if !text.is_empty() {
                        channel
                            .display_name
                            .push(EpgStringWithLang { value: text, lang });
                    }
                }
                b"url" => {
                    let system = get_attr(e, b"system");
                    let text = read_text_content(reader)?;
                    if !text.is_empty() {
                        let url = EpgUrl {
                            value: text,
                            system,
                        };
                        channel.urls.push(url.clone());
                        if channel.url.is_none() {
                            channel.url = Some(url);
                        }
                    }
                }
                _ => {
                    depth += 1;
                }
            },
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"icon" => {
                let icon = parse_icon_attrs(e);
                channel.icons.push(icon.clone());
                // Backward compat: first icon also fills `channel.icon`.
                if channel.icon.is_none() {
                    channel.icon = Some(icon);
                }
            }
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(channel)
}

// ── Programme parsing ───────────────────────────────────────────

fn parse_programme_start(e: &BytesStart<'_>) -> Result<EpgProgramme, XmltvError> {
    let channel = get_required_attr(e, b"channel", "programme")?;
    let start_raw = get_required_attr(e, b"start", "programme")?;
    let stop = parse_optional_timestamp_attr(e, b"stop", "programme stop")?;

    Ok(EpgProgramme {
        channel,
        start: Some(
            try_parse_xmltv_timestamp(&start_raw).map_err(|_| {
                XmltvError::Timestamp(format!(
                    "programme start `{start_raw}` does not follow the supported XMLTV timestamp grammar"
                ))
            })?,
        ),
        stop,
        pdc_start: parse_optional_raw_timestamp_attr(e, b"pdc-start", "programme pdc-start")?,
        vps_start: parse_optional_raw_timestamp_attr(e, b"vps-start", "programme vps-start")?,
        showview: get_attr(e, b"showview"),
        videoplus: get_attr(e, b"videoplus"),
        clumpidx: get_attr(e, b"clumpidx"),
        ..Default::default()
    })
}

fn parse_programme_body<R: BufRead>(
    reader: &mut Reader<R>,
    mut prog: EpgProgramme,
) -> Result<EpgProgramme, XmltvError> {
    let mut depth: u32 = 1;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"title" => {
                    let lang = get_attr(e, b"lang");
                    let value = read_text_content(reader)?;
                    prog.title.push(EpgStringWithLang { value, lang });
                }
                b"sub-title" => {
                    let lang = get_attr(e, b"lang");
                    let value = read_text_content(reader)?;
                    prog.sub_title.push(EpgStringWithLang { value, lang });
                }
                b"desc" => {
                    let lang = get_attr(e, b"lang");
                    let value = read_text_content(reader)?;
                    prog.desc.push(EpgStringWithLang { value, lang });
                }
                b"credits" => {
                    prog.credits = Some(parse_credits(reader)?);
                }
                b"date" => {
                    let text = read_text_content(reader)?;
                    if !text.is_empty() {
                        prog.date = Some(text);
                    }
                }
                b"category" => {
                    let lang = get_attr(e, b"lang");
                    let value = read_text_content(reader)?;
                    prog.category.push(EpgStringWithLang { value, lang });
                }
                b"keyword" => {
                    let lang = get_attr(e, b"lang");
                    let value = read_text_content(reader)?;
                    prog.keyword.push(EpgStringWithLang { value, lang });
                }
                b"language" => {
                    let lang = get_attr(e, b"lang");
                    let value = read_text_content(reader)?;
                    prog.language.push(EpgStringWithLang { value, lang });
                }
                b"orig-language" => {
                    let lang = get_attr(e, b"lang");
                    let value = read_text_content(reader)?;
                    prog.orig_language = Some(EpgStringWithLang { value, lang });
                }
                b"length" => {
                    prog.length = Some(parse_length(reader, e)?);
                }
                b"url" => {
                    let system = get_attr(e, b"system");
                    let value = read_text_content(reader)?;
                    if !value.is_empty() {
                        prog.url.push(EpgUrl { value, system });
                    }
                }
                b"country" => {
                    let lang = get_attr(e, b"lang");
                    let value = read_text_content(reader)?;
                    prog.country.push(EpgStringWithLang { value, lang });
                }
                b"episode-num" => {
                    let system = get_attr(e, b"system");
                    let value = read_text_content(reader)?;
                    prog.episode_num.push(EpgEpisodeNumber { value, system });
                }
                b"image" => {
                    let image_type = get_attr(e, b"type");
                    let size = get_attr(e, b"size");
                    let orient = get_attr(e, b"orient");
                    let system = get_attr(e, b"system");
                    let url = read_text_content(reader)?;
                    prog.image.push(EpgImage {
                        url,
                        image_type,
                        size,
                        orient,
                        system,
                    });
                }
                b"rating" => {
                    let system = get_attr(e, b"system");
                    prog.rating.push(parse_rating(reader, system)?);
                }
                b"star-rating" => {
                    let system = get_attr(e, b"system");
                    prog.star_rating.push(parse_rating(reader, system)?);
                }
                b"premiere" => {
                    prog.is_premiere = true;
                    let value = read_text_content(reader)?;
                    let lang = get_attr(e, b"lang");
                    if !value.is_empty() || lang.is_some() {
                        prog.premiere = Some(EpgStringWithLang { value, lang });
                    }
                }
                b"last-chance" => {
                    prog.is_last_chance = true;
                    let value = read_text_content(reader)?;
                    let lang = get_attr(e, b"lang");
                    if !value.is_empty() || lang.is_some() {
                        prog.last_chance = Some(EpgStringWithLang { value, lang });
                    }
                }
                b"video" => {
                    prog.video = Some(parse_video(reader)?);
                }
                b"audio" => {
                    prog.audio.push(parse_audio(reader)?);
                }
                b"subtitles" => {
                    prog.subtitles.push(parse_subtitles(reader, e)?);
                }
                b"review" => {
                    let review_type = get_attr(e, b"type");
                    let source = get_attr(e, b"source");
                    let reviewer = get_attr(e, b"reviewer");
                    let lang = get_attr(e, b"lang");
                    let value = read_text_content(reader)?;
                    prog.review.push(EpgReview {
                        value,
                        review_type,
                        source,
                        reviewer,
                        lang,
                    });
                }
                _ => {
                    depth += 1;
                }
            },
            Ok(Event::Empty(ref e)) => match e.name().as_ref() {
                b"icon" => {
                    prog.icon = Some(parse_icon_attrs(e));
                }
                b"new" => {
                    prog.is_new = true;
                }
                b"previously-shown" => {
                    prog.is_rerun = true;
                    prog.previously_shown = Some(parse_previously_shown(e)?);
                }
                b"premiere" => {
                    prog.is_premiere = true;
                }
                b"last-chance" => {
                    prog.is_last_chance = true;
                }
                b"subtitles" => {
                    prog.subtitles.push(parse_empty_subtitles(e)?);
                }
                b"review" => {
                    // Empty <review/> — unlikely but handle gracefully.
                    prog.review.push(EpgReview {
                        review_type: get_attr(e, b"type"),
                        source: get_attr(e, b"source"),
                        reviewer: get_attr(e, b"reviewer"),
                        lang: get_attr(e, b"lang"),
                        ..Default::default()
                    });
                }
                _ => {}
            },
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    if prog.title.is_empty() {
        return Err(XmltvError::Validation(
            "programme is missing the required <title> element".into(),
        ));
    }

    Ok(prog)
}

// ── Credits parsing ─────────────────────────────────────────────

fn parse_credits<R: BufRead>(reader: &mut Reader<R>) -> Result<EpgCredits, XmltvError> {
    let mut credits = EpgCredits::default();
    let mut depth: u32 = 1;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"director" => {
                    credits
                        .director
                        .push(parse_credit_person(reader, None, false)?);
                }
                b"actor" => {
                    let role = get_attr(e, b"role");
                    let guest = get_attr(e, b"guest")
                        .map(|v| v.eq_ignore_ascii_case("yes"))
                        .unwrap_or(false);
                    credits
                        .actor
                        .push(parse_credit_person(reader, role, guest)?);
                }
                b"writer" => {
                    credits
                        .writer
                        .push(parse_credit_person(reader, None, false)?);
                }
                b"adapter" => {
                    credits
                        .adapter
                        .push(parse_credit_person(reader, None, false)?);
                }
                b"producer" => {
                    credits
                        .producer
                        .push(parse_credit_person(reader, None, false)?);
                }
                b"composer" => {
                    credits
                        .composer
                        .push(parse_credit_person(reader, None, false)?);
                }
                b"editor" => {
                    credits
                        .editor
                        .push(parse_credit_person(reader, None, false)?);
                }
                b"presenter" => {
                    credits
                        .presenter
                        .push(parse_credit_person(reader, None, false)?);
                }
                b"commentator" => {
                    credits
                        .commentator
                        .push(parse_credit_person(reader, None, false)?);
                }
                b"guest" => {
                    credits.guest.push(parse_credit_person(reader, None, true)?);
                }
                _ => {
                    depth += 1;
                }
            },
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(credits)
}

// ── Rating value parsing ────────────────────────────────────────

/// Parse a `<rating>` or `<star-rating>` body, extracting the `<value>` child
/// and any nested `<icon/>` elements.
fn parse_rating<R: BufRead>(
    reader: &mut Reader<R>,
    system: Option<String>,
) -> Result<EpgRating, XmltvError> {
    let mut value = String::new();
    let mut icons = SmallVec::new();
    let mut depth: u32 = 1;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"value" => {
                value = read_text_content(reader)?;
            }
            Ok(Event::Empty(ref e)) if e.name().as_ref() == b"icon" => {
                icons.push(parse_icon_attrs(e));
            }
            Ok(Event::Start(_)) => {
                depth += 1;
            }
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(EpgRating {
        value,
        system,
        icons,
    })
}

// ── Video parsing ───────────────────────────────────────────────

/// Parse a `<video>` body with children `<present>`, `<colour>`, `<aspect>`, `<quality>`.
fn parse_video<R: BufRead>(reader: &mut Reader<R>) -> Result<EpgVideo, XmltvError> {
    let mut video = EpgVideo::default();
    let mut depth: u32 = 1;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"present" => {
                    let text = read_text_content(reader)?;
                    video.present = Some(text.eq_ignore_ascii_case("yes"));
                }
                b"colour" => {
                    let text = read_text_content(reader)?;
                    video.colour = Some(text.eq_ignore_ascii_case("yes"));
                }
                b"aspect" => {
                    video.aspect = Some(read_text_content(reader)?);
                }
                b"quality" => {
                    video.quality = Some(read_text_content(reader)?);
                }
                _ => {
                    depth += 1;
                }
            },
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(video)
}

// ── Audio parsing ───────────────────────────────────────────────

/// Parse an `<audio>` body with children `<present>`, `<stereo>`.
fn parse_audio<R: BufRead>(reader: &mut Reader<R>) -> Result<EpgAudio, XmltvError> {
    let mut audio = EpgAudio::default();
    let mut depth: u32 = 1;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name().as_ref() {
                b"present" => {
                    let text = read_text_content(reader)?;
                    audio.present = Some(text.eq_ignore_ascii_case("yes"));
                }
                b"stereo" => {
                    audio.stereo = Some(read_text_content(reader)?);
                }
                _ => {
                    depth += 1;
                }
            },
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(audio)
}

fn parse_credit_person<R: BufRead>(
    reader: &mut Reader<R>,
    role: Option<String>,
    guest: bool,
) -> Result<EpgPerson, XmltvError> {
    let mut person = EpgPerson {
        role,
        guest,
        ..Default::default()
    };
    let mut depth: u32 = 1;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref e)) => {
                if let Ok(t) = e.unescape() {
                    person.name.push_str(&t);
                }
            }
            Ok(Event::CData(ref e)) => {
                if let Ok(t) = std::str::from_utf8(e.as_ref()) {
                    person.name.push_str(t);
                }
            }
            Ok(Event::Start(ref e)) if depth == 1 && e.name().as_ref() == b"image" => {
                let image = read_text_content(reader)?;
                if !image.is_empty() {
                    person.images.push(image);
                }
            }
            Ok(Event::Start(ref e)) if depth == 1 && e.name().as_ref() == b"url" => {
                let url = read_text_content(reader)?;
                if !url.is_empty() {
                    person.urls.push(url);
                }
            }
            Ok(Event::Start(_)) => {
                depth += 1;
            }
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(person)
}

fn parse_length<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
) -> Result<EpgLength, XmltvError> {
    let units = parse_length_unit_attr(get_attr(e, b"units").as_deref())?;
    let value = read_text_content(reader)?;
    let value = value.parse::<u32>().map_err(|_| {
        XmltvError::Xml(format!(
            "length value `{value}` is not a valid unsigned integer"
        ))
    })?;

    Ok(EpgLength { value, units })
}

fn parse_length_unit_attr(units: Option<&str>) -> Result<EpgLengthUnit, XmltvError> {
    match units.unwrap_or("minutes") {
        "seconds" => Ok(EpgLengthUnit::Seconds),
        "minutes" => Ok(EpgLengthUnit::Minutes),
        "hours" => Ok(EpgLengthUnit::Hours),
        other => Err(XmltvError::Xml(format!(
            "length units `{other}` must be one of `seconds`, `minutes`, or `hours`"
        ))),
    }
}

fn parse_subtitles<R: BufRead>(
    reader: &mut Reader<R>,
    e: &BytesStart<'_>,
) -> Result<EpgSubtitles, XmltvError> {
    let mut subtitles = parse_empty_subtitles(e)?;
    let mut depth: u32 = 1;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"language" => {
                let lang = get_attr(e, b"lang");
                let value = read_text_content(reader)?;
                subtitles.language = Some(EpgStringWithLang { value, lang });
            }
            Ok(Event::Start(_)) => {
                depth += 1;
            }
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(subtitles)
}

fn parse_empty_subtitles(e: &BytesStart<'_>) -> Result<EpgSubtitles, XmltvError> {
    Ok(EpgSubtitles {
        subtitle_type: parse_subtitle_type(get_attr(e, b"type").as_deref())?,
        language: None,
    })
}

fn parse_subtitle_type(value: Option<&str>) -> Result<Option<EpgSubtitleType>, XmltvError> {
    match value {
        None => Ok(None),
        Some("teletext") => Ok(Some(EpgSubtitleType::Teletext)),
        Some("onscreen") => Ok(Some(EpgSubtitleType::Onscreen)),
        Some("deaf-signed") => Ok(Some(EpgSubtitleType::DeafSigned)),
        Some(other) => Err(XmltvError::Xml(format!(
            "subtitle type `{other}` must be `teletext`, `onscreen`, or `deaf-signed`"
        ))),
    }
}

fn parse_previously_shown(e: &BytesStart<'_>) -> Result<EpgPreviouslyShown, XmltvError> {
    Ok(EpgPreviouslyShown {
        start: parse_optional_raw_timestamp_attr(e, b"start", "previously-shown start")?,
        channel: get_attr(e, b"channel"),
    })
}

// ── Icon attribute parsing ──────────────────────────────────────

fn parse_icon_attrs(e: &BytesStart<'_>) -> EpgIcon {
    EpgIcon {
        src: get_attr(e, b"src").unwrap_or_default(),
        width: get_attr(e, b"width").and_then(|v| v.parse().ok()),
        height: get_attr(e, b"height").and_then(|v| v.parse().ok()),
    }
}

// ── Helpers ─────────────────────────────────────────────────────

fn get_required_attr(e: &BytesStart<'_>, key: &[u8], context: &str) -> Result<String, XmltvError> {
    get_attr(e, key)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            XmltvError::Validation(format!(
                "{context} is missing the required `{}` attribute",
                String::from_utf8_lossy(key)
            ))
        })
}

fn parse_optional_timestamp_attr(
    e: &BytesStart<'_>,
    key: &[u8],
    context: &str,
) -> Result<Option<i64>, XmltvError> {
    get_attr(e, key)
        .map(|raw| {
            try_parse_xmltv_timestamp(&raw).map_err(|_| {
                XmltvError::Timestamp(format!(
                    "{context} `{raw}` does not follow the supported XMLTV timestamp grammar"
                ))
            })
        })
        .transpose()
}

fn parse_optional_raw_timestamp_attr(
    e: &BytesStart<'_>,
    key: &[u8],
    context: &str,
) -> Result<Option<String>, XmltvError> {
    get_attr(e, key)
        .map(|raw| {
            validate_xmltv_timestamp(&raw).map_err(|_| {
                XmltvError::Timestamp(format!(
                    "{context} `{raw}` does not follow the supported XMLTV timestamp grammar"
                ))
            })?;
            Ok(raw)
        })
        .transpose()
}

/// Read text content until the closing tag of the current element.
///
/// Consumes all events until the matching `End` event, collecting text.
/// Uses its own buffer to avoid borrow conflicts with the caller.
fn read_text_content<R: BufRead>(reader: &mut Reader<R>) -> Result<String, XmltvError> {
    let mut text = String::new();
    let mut depth: u32 = 1;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Text(ref e)) => {
                if let Ok(t) = e.unescape() {
                    text.push_str(&t);
                }
            }
            Ok(Event::CData(ref e)) => {
                if let Ok(t) = std::str::from_utf8(e.as_ref()) {
                    text.push_str(t);
                }
            }
            Ok(Event::Start(_)) => {
                depth += 1;
            }
            Ok(Event::End(_)) => {
                if depth <= 1 {
                    break;
                }
                depth -= 1;
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(XmltvError::Xml(e.to_string())),
            _ => {}
        }
        buf.clear();
    }

    Ok(text)
}

/// Extract a UTF-8 attribute value from an XML element by key.
fn get_attr(e: &BytesStart<'_>, key: &[u8]) -> Option<String> {
    for attr in e.attributes().flatten() {
        if attr.key.as_ref() == key {
            return attr.unescape_value().ok().map(std::borrow::Cow::into_owned);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crispy_iptv_types::epg::{EpgLengthUnit, EpgSubtitleType};

    const MINIMAL_XMLTV: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE tv SYSTEM "xmltv.dtd">
<tv>
  <channel id="ch1">
    <display-name>Channel One</display-name>
    <icon src="https://example.com/ch1.png" width="100" height="50"/>
    <url>https://example.com/ch1</url>
  </channel>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title lang="en">Test Show</title>
    <desc lang="en">A test description.</desc>
  </programme>
</tv>"#;

    #[test]
    fn parse_minimal_xmltv() {
        let doc = parse(MINIMAL_XMLTV).unwrap();
        assert_eq!(doc.channels.len(), 1);
        assert_eq!(doc.programmes.len(), 1);

        let ch = &doc.channels[0];
        assert_eq!(ch.id, "ch1");
        assert_eq!(ch.display_name[0].value, "Channel One");
        assert_eq!(ch.icon.as_ref().unwrap().src, "https://example.com/ch1.png");
        assert_eq!(ch.icon.as_ref().unwrap().width, Some(100));
        assert_eq!(
            ch.url.as_ref().map(|u| u.value.as_str()),
            Some("https://example.com/ch1")
        );

        let prog = &doc.programmes[0];
        assert_eq!(prog.channel, "ch1");
        assert!(prog.start.is_some());
        assert!(prog.stop.is_some());
        assert_eq!(prog.title[0].value, "Test Show");
        assert_eq!(prog.title[0].lang.as_deref(), Some("en"));
        assert_eq!(prog.desc[0].value, "A test description.");
    }

    #[test]
    fn parse_multilingual_display_names() {
        let xml = r#"<tv>
  <channel id="bbc1">
    <display-name lang="en">BBC One</display-name>
    <display-name lang="cy">BBC Un</display-name>
    <display-name lang="gd">BBC Aon</display-name>
  </channel>
</tv>"#;
        let doc = parse(xml).unwrap();
        let ch = &doc.channels[0];
        assert_eq!(ch.display_name.len(), 3);
        assert_eq!(ch.display_name[0].value, "BBC One");
        assert_eq!(ch.display_name[0].lang.as_deref(), Some("en"));
        assert_eq!(ch.display_name[1].value, "BBC Un");
        assert_eq!(ch.display_name[1].lang.as_deref(), Some("cy"));
        assert_eq!(ch.display_name[2].value, "BBC Aon");
        assert_eq!(ch.display_name[2].lang.as_deref(), Some("gd"));
    }

    #[test]
    fn parse_full_credits() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Movie</title>
    <credits>
      <director>Steven Spielberg</director>
      <director>James Cameron</director>
      <actor role="Hero">Tom Hanks</actor>
      <actor>Meryl Streep</actor>
      <writer>Aaron Sorkin</writer>
      <producer>Kathleen Kennedy</producer>
      <composer>John Williams</composer>
      <presenter>Ryan Seacrest</presenter>
      <commentator>John Madden</commentator>
      <guest>Oprah Winfrey</guest>
    </credits>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let credits = doc.programmes[0].credits.as_ref().unwrap();

        assert_eq!(credits.director.len(), 2);
        assert_eq!(credits.director[0].name, "Steven Spielberg");
        assert_eq!(credits.director[1].name, "James Cameron");

        assert_eq!(credits.actor.len(), 2);
        assert_eq!(credits.actor[0].name, "Tom Hanks");
        assert_eq!(credits.actor[0].role.as_deref(), Some("Hero"));
        assert_eq!(credits.actor[1].name, "Meryl Streep");
        assert!(credits.actor[1].role.is_none());

        assert_eq!(credits.writer[0].name, "Aaron Sorkin");
        assert_eq!(credits.producer[0].name, "Kathleen Kennedy");
        assert_eq!(credits.composer[0].name, "John Williams");
        assert_eq!(credits.presenter[0].name, "Ryan Seacrest");
        assert_eq!(credits.commentator[0].name, "John Madden");

        assert_eq!(credits.guest.len(), 1);
        assert_eq!(credits.guest[0].name, "Oprah Winfrey");
        assert!(credits.guest[0].guest);
    }

    #[test]
    fn parse_episode_numbers() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Series</title>
    <episode-num system="xmltv_ns">2.5.0/1</episode-num>
    <episode-num system="onscreen">S03E06</episode-num>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let prog = &doc.programmes[0];

        assert_eq!(prog.episode_num.len(), 2);
        assert_eq!(prog.episode_num[0].value, "2.5.0/1");
        assert_eq!(prog.episode_num[0].system.as_deref(), Some("xmltv_ns"));
        assert_eq!(prog.episode_num[1].value, "S03E06");
        assert_eq!(prog.episode_num[1].system.as_deref(), Some("onscreen"));
    }

    #[test]
    fn parse_ratings_and_star_ratings() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Rated Show</title>
    <rating system="MPAA">
      <value>PG-13</value>
    </rating>
    <rating system="VCHIP">
      <value>TV-14</value>
    </rating>
    <star-rating system="imdb">
      <value>8.5/10</value>
    </star-rating>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let prog = &doc.programmes[0];

        assert_eq!(prog.rating.len(), 2);
        assert_eq!(prog.rating[0].value, "PG-13");
        assert_eq!(prog.rating[0].system.as_deref(), Some("MPAA"));
        assert_eq!(prog.rating[1].value, "TV-14");
        assert_eq!(prog.rating[1].system.as_deref(), Some("VCHIP"));

        assert_eq!(prog.star_rating.len(), 1);
        assert_eq!(prog.star_rating[0].value, "8.5/10");
        assert_eq!(prog.star_rating[0].system.as_deref(), Some("imdb"));
    }

    #[test]
    fn parse_boolean_flags() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>New Show</title>
    <new/>
    <premiere/>
  </programme>
  <programme start="20250115130000 +0000" stop="20250115140000 +0000" channel="ch1">
    <title>Rerun</title>
    <previously-shown/>
    <last-chance/>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();

        let new_prog = &doc.programmes[0];
        assert!(new_prog.is_new);
        assert!(new_prog.is_premiere);
        assert!(!new_prog.is_rerun);
        assert!(!new_prog.is_last_chance);

        let rerun_prog = &doc.programmes[1];
        assert!(!rerun_prog.is_new);
        assert!(!rerun_prog.is_premiere);
        assert!(rerun_prog.is_rerun);
        assert!(rerun_prog.is_last_chance);
    }

    #[test]
    fn parse_boolean_flags_with_text_content() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Show</title>
    <premiere>First showing</premiere>
    <last-chance>Last chance to watch</last-chance>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let prog = &doc.programmes[0];
        assert!(prog.is_premiere);
        assert!(prog.is_last_chance);
        assert_eq!(
            prog.premiere.as_ref().map(|p| p.value.as_str()),
            Some("First showing")
        );
        assert_eq!(
            prog.last_chance.as_ref().map(|p| p.value.as_str()),
            Some("Last chance to watch")
        );
    }

    #[test]
    fn parse_images_and_icons() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Show</title>
    <image type="poster" size="3" orient="P">https://example.com/poster.jpg</image>
    <image type="backdrop">https://example.com/backdrop.jpg</image>
    <icon src="https://example.com/thumb.png" width="200" height="300"/>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let prog = &doc.programmes[0];

        assert_eq!(prog.image.len(), 2);
        assert_eq!(prog.image[0].url, "https://example.com/poster.jpg");
        assert_eq!(prog.image[0].image_type.as_deref(), Some("poster"));
        assert_eq!(prog.image[0].size.as_deref(), Some("3"));
        assert_eq!(prog.image[0].orient.as_deref(), Some("P"));
        assert_eq!(prog.image[1].url, "https://example.com/backdrop.jpg");
        assert_eq!(prog.image[1].image_type.as_deref(), Some("backdrop"));

        let icon = prog.icon.as_ref().unwrap();
        assert_eq!(icon.src, "https://example.com/thumb.png");
        assert_eq!(icon.width, Some(200));
        assert_eq!(icon.height, Some(300));
    }

    #[test]
    fn parse_from_bufreader() {
        let cursor = std::io::Cursor::new(MINIMAL_XMLTV.as_bytes());
        let reader = std::io::BufReader::new(cursor);
        let doc = parse_reader(reader).unwrap();
        assert_eq!(doc.channels.len(), 1);
        assert_eq!(doc.programmes.len(), 1);
        assert_eq!(doc.channels[0].id, "ch1");
        assert_eq!(doc.programmes[0].title[0].value, "Test Show");
    }

    #[test]
    fn handle_malformed_xml_gracefully() {
        let bad_xml = r#"<tv>
  <channel id="ch1">
    <display-name>Test</display-name>
  </channel>
  <programme start="20250115120000 +0000" channel="ch1">
    <title>Show</title>
  </programme>
  <!-- Missing closing </tv> tag -->"#;

        let result = parse(bad_xml);
        match result {
            Ok(doc) => {
                assert_eq!(doc.channels.len(), 1);
                assert_eq!(doc.programmes.len(), 1);
            }
            Err(_) => {
                // Also acceptable — graceful error, no panic.
            }
        }
    }

    #[test]
    fn handle_truly_broken_xml() {
        let broken = "<<<not xml at all>>>";
        let result = parse(broken);
        // Must not panic.
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn parse_empty_document() {
        let xml = r#"<?xml version="1.0"?><tv></tv>"#;
        let doc = parse(xml).unwrap();
        assert!(doc.channels.is_empty());
        assert!(doc.programmes.is_empty());
    }

    #[test]
    fn extract_channel_names_works() {
        let names = extract_channel_names(MINIMAL_XMLTV).unwrap();
        assert_eq!(names.get("ch1").unwrap(), "Channel One");
    }

    #[test]
    fn parse_programme_with_subtitle_and_categories() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title lang="en">Main Title</title>
    <sub-title lang="en">The Subtitle</sub-title>
    <category lang="en">Drama</category>
    <category lang="en">Thriller</category>
    <date>2025</date>
    <length units="hours">2</length>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let prog = &doc.programmes[0];

        assert_eq!(prog.sub_title[0].value, "The Subtitle");
        assert_eq!(prog.category.len(), 2);
        assert_eq!(prog.category[0].value, "Drama");
        assert_eq!(prog.category[1].value, "Thriller");
        assert_eq!(prog.date.as_deref(), Some("2025"));
        assert_eq!(prog.length.as_ref().unwrap().value, 2);
        assert_eq!(prog.length.as_ref().unwrap().units, EpgLengthUnit::Hours);
    }

    #[test]
    fn parse_length_without_units_defaults_to_minutes() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" channel="ch1">
    <title>Show</title>
    <length>60</length>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let length = doc.programmes[0].length.as_ref().unwrap();
        assert_eq!(length.value, 60);
        assert_eq!(length.units, EpgLengthUnit::Minutes);
    }

    #[test]
    fn parse_rejects_invalid_programme_timestamps() {
        let bad_start = r#"<tv>
  <programme start="20250115120000 +0000junk" channel="ch1">
    <title>Show</title>
  </programme>
</tv>"#;
        assert!(matches!(parse(bad_start), Err(XmltvError::Timestamp(_))));

        let missing_start = r#"<tv>
  <programme channel="ch1">
    <title>Show</title>
  </programme>
</tv>"#;
        assert!(matches!(
            parse(missing_start),
            Err(XmltvError::Validation(_))
        ));
    }

    #[test]
    fn parse_programme_accepts_named_timezone_suffixes() {
        let xml = r#"<tv>
  <programme start="200007281733 BST" channel="ch1">
    <title>Show</title>
  </programme>
</tv>"#;

        let doc = parse(xml).unwrap();
        let expected = try_parse_xmltv_timestamp("200007281733 +0100").unwrap();
        assert_eq!(doc.programmes[0].start, Some(expected));
    }

    #[test]
    fn parse_programme_subtitles_and_previous_showing() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" channel="ch1">
    <title>Show</title>
    <previously-shown start="20240115120000 +0000" channel="archive"/>
    <subtitles type="onscreen">
      <language lang="en">English</language>
    </subtitles>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let prog = &doc.programmes[0];
        assert!(prog.is_rerun);
        assert_eq!(
            prog.previously_shown
                .as_ref()
                .and_then(|shown| shown.channel.as_deref()),
            Some("archive")
        );
        assert_eq!(prog.subtitles.len(), 1);
        assert_eq!(
            prog.subtitles[0].subtitle_type,
            Some(EpgSubtitleType::Onscreen)
        );
        assert_eq!(
            prog.subtitles[0]
                .language
                .as_ref()
                .map(|language| language.value.as_str()),
            Some("English")
        );
    }

    #[test]
    fn parse_cdata_content() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title><![CDATA[Show <with> special & chars]]></title>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        assert_eq!(
            doc.programmes[0].title[0].value,
            "Show <with> special & chars"
        );
    }

    #[test]
    fn parse_xml_entities() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Rock &amp; Roll</title>
    <desc>A &quot;great&quot; show</desc>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.programmes[0].title[0].value, "Rock & Roll");
        assert_eq!(doc.programmes[0].desc[0].value, "A \"great\" show");
    }

    #[test]
    fn parse_keyword() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Show</title>
    <keyword lang="en">Drama</keyword>
    <keyword lang="fr">Drame</keyword>
    <keyword>Thriller</keyword>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let prog = &doc.programmes[0];
        assert_eq!(prog.keyword.len(), 3);
        assert_eq!(prog.keyword[0].value, "Drama");
        assert_eq!(prog.keyword[0].lang.as_deref(), Some("en"));
        assert_eq!(prog.keyword[1].value, "Drame");
        assert_eq!(prog.keyword[1].lang.as_deref(), Some("fr"));
        assert_eq!(prog.keyword[2].value, "Thriller");
        assert!(prog.keyword[2].lang.is_none());
    }

    #[test]
    fn parse_language_element() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Show</title>
    <language lang="en">English</language>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.programmes[0].language[0].value, "English");
        assert_eq!(doc.programmes[0].language[0].lang.as_deref(), Some("en"));
    }

    #[test]
    fn parse_orig_language() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Show</title>
    <orig-language lang="fr">French</orig-language>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let prog = &doc.programmes[0];
        let ol = prog.orig_language.as_ref().unwrap();
        assert_eq!(ol.value, "French");
        assert_eq!(ol.lang.as_deref(), Some("fr"));
    }

    #[test]
    fn parse_video_element() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>HD Show</title>
    <video>
      <present>yes</present>
      <colour>yes</colour>
      <aspect>16:9</aspect>
      <quality>HDTV</quality>
    </video>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let video = doc.programmes[0].video.as_ref().unwrap();
        assert_eq!(video.present, Some(true));
        assert_eq!(video.colour, Some(true));
        assert_eq!(video.aspect.as_deref(), Some("16:9"));
        assert_eq!(video.quality.as_deref(), Some("HDTV"));
    }

    #[test]
    fn parse_video_partial() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Show</title>
    <video><aspect>16:9</aspect><quality>HDTV</quality></video>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let video = doc.programmes[0].video.as_ref().unwrap();
        assert!(video.present.is_none());
        assert!(video.colour.is_none());
        assert_eq!(video.aspect.as_deref(), Some("16:9"));
        assert_eq!(video.quality.as_deref(), Some("HDTV"));
    }

    #[test]
    fn parse_audio_element() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Show</title>
    <audio>
      <present>yes</present>
      <stereo>surround</stereo>
    </audio>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        assert_eq!(doc.programmes[0].audio.len(), 1);
        let audio = &doc.programmes[0].audio[0];
        assert_eq!(audio.present, Some(true));
        assert_eq!(audio.stereo.as_deref(), Some("surround"));
    }

    #[test]
    fn parse_audio_stereo_only() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Show</title>
    <audio><stereo>dolby digital</stereo></audio>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let audio = &doc.programmes[0].audio[0];
        assert!(audio.present.is_none());
        assert_eq!(audio.stereo.as_deref(), Some("dolby digital"));
    }

    #[test]
    fn parse_review_element() {
        let xml = r#"<tv>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Show</title>
    <review type="text" source="NYT" reviewer="Jane Doe" lang="en">Great show</review>
    <review type="url">https://example.com/review</review>
  </programme>
</tv>"#;
        let doc = parse(xml).unwrap();
        let prog = &doc.programmes[0];
        assert_eq!(prog.review.len(), 2);

        assert_eq!(prog.review[0].value, "Great show");
        assert_eq!(prog.review[0].review_type.as_deref(), Some("text"));
        assert_eq!(prog.review[0].source.as_deref(), Some("NYT"));
        assert_eq!(prog.review[0].reviewer.as_deref(), Some("Jane Doe"));
        assert_eq!(prog.review[0].lang.as_deref(), Some("en"));

        assert_eq!(prog.review[1].value, "https://example.com/review");
        assert_eq!(prog.review[1].review_type.as_deref(), Some("url"));
        assert!(prog.review[1].source.is_none());
    }

    #[test]
    fn parse_channel_multiple_urls() {
        let xml = r#"<tv>
  <channel id="ch1">
    <display-name>Channel One</display-name>
    <url>https://example.com</url>
    <url>https://mirror.example.com</url>
  </channel>
</tv>"#;
        let doc = parse(xml).unwrap();
        let ch = &doc.channels[0];
        assert_eq!(ch.urls.len(), 2);
        assert_eq!(ch.urls[0].value, "https://example.com");
        assert_eq!(ch.urls[1].value, "https://mirror.example.com");
        // Backward compat: first URL in `url`.
        assert_eq!(
            ch.url.as_ref().map(|u| u.value.as_str()),
            Some("https://example.com")
        );
    }

    #[test]
    fn parse_channel_multiple_icons() {
        let xml = r#"<tv>
  <channel id="ch1">
    <display-name>Channel One</display-name>
    <icon src="https://example.com/logo1.png" width="100" height="50"/>
    <icon src="https://example.com/logo2.png"/>
  </channel>
</tv>"#;
        let doc = parse(xml).unwrap();
        let ch = &doc.channels[0];
        assert_eq!(ch.icons.len(), 2);
        assert_eq!(ch.icons[0].src, "https://example.com/logo1.png");
        assert_eq!(ch.icons[0].width, Some(100));
        assert_eq!(ch.icons[1].src, "https://example.com/logo2.png");
        assert!(ch.icons[1].width.is_none());
        // Backward compat: first icon in `icon`.
        assert_eq!(
            ch.icon.as_ref().unwrap().src,
            "https://example.com/logo1.png"
        );
    }
}
