//! XMLTV writer — serializes `XmltvDocument` to valid XMLTV XML.

use crispy_iptv_types::epg::{
    EpgAudio, EpgChannel, EpgCredits, EpgIcon, EpgLength, EpgLengthUnit, EpgPerson, EpgProgramme,
    EpgRating, EpgReview, EpgStringWithLang, EpgSubtitles, EpgUrl, EpgVideo,
};

use crate::error::XmltvError;
use crate::timestamp::{format_xmltv_timestamp, validate_xmltv_timestamp};
use crate::types::XmltvDocument;

/// Write an `XmltvDocument` to XMLTV XML.
///
/// Serialization is fallible because XMLTV requires certain programme fields
/// and timestamp values to be valid before they can be emitted.
pub fn write(doc: &XmltvDocument) -> Result<String, XmltvError> {
    let mut out = String::with_capacity(4096);

    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<!DOCTYPE tv SYSTEM \"xmltv.dtd\">\n");
    out.push_str("<tv>\n");

    for channel in &doc.channels {
        write_channel(&mut out, channel);
    }

    for programme in &doc.programmes {
        write_programme(&mut out, programme)?;
    }

    out.push_str("</tv>\n");
    Ok(out)
}

fn write_channel(out: &mut String, ch: &EpgChannel) {
    out.push_str("  <channel id=\"");
    write_escaped(out, &ch.id);
    out.push_str("\">\n");

    for dn in &ch.display_name {
        write_string_with_lang(out, "display-name", dn);
    }

    if !ch.icons.is_empty() {
        for icon in &ch.icons {
            write_icon(out, icon);
        }
    } else if let Some(ref icon) = ch.icon {
        write_icon(out, icon);
    }

    if !ch.urls.is_empty() {
        for url in &ch.urls {
            write_url(out, url);
        }
    } else if let Some(ref url) = ch.url {
        write_url(out, url);
    }

    out.push_str("  </channel>\n");
}

fn write_programme(out: &mut String, prog: &EpgProgramme) -> Result<(), XmltvError> {
    validate_programme(prog)?;

    out.push_str("  <programme");
    out.push_str(" start=\"");
    out.push_str(&format_xmltv_timestamp(
        prog.start.expect("validated programme start is present"),
    )?);
    out.push('"');

    if let Some(stop) = prog.stop {
        out.push_str(" stop=\"");
        out.push_str(&format_xmltv_timestamp(stop)?);
        out.push('"');
    }
    if let Some(ref pdc_start) = prog.pdc_start {
        out.push_str(" pdc-start=\"");
        write_escaped(out, pdc_start);
        out.push('"');
    }
    if let Some(ref vps_start) = prog.vps_start {
        out.push_str(" vps-start=\"");
        write_escaped(out, vps_start);
        out.push('"');
    }
    if let Some(ref showview) = prog.showview {
        out.push_str(" showview=\"");
        write_escaped(out, showview);
        out.push('"');
    }
    if let Some(ref videoplus) = prog.videoplus {
        out.push_str(" videoplus=\"");
        write_escaped(out, videoplus);
        out.push('"');
    }
    out.push_str(" channel=\"");
    write_escaped(out, &prog.channel);
    out.push('"');
    if let Some(ref clumpidx) = prog.clumpidx {
        out.push_str(" clumpidx=\"");
        write_escaped(out, clumpidx);
        out.push('"');
    }
    out.push_str(">\n");

    for title in &prog.title {
        write_string_with_lang(out, "title", title);
    }
    for sub_title in &prog.sub_title {
        write_string_with_lang(out, "sub-title", sub_title);
    }
    for desc in &prog.desc {
        write_string_with_lang(out, "desc", desc);
    }

    if let Some(ref credits) = prog.credits {
        write_credits(out, credits);
    }

    if let Some(ref date) = prog.date {
        out.push_str("    <date>");
        write_escaped(out, date);
        out.push_str("</date>\n");
    }
    for category in &prog.category {
        write_string_with_lang(out, "category", category);
    }
    for keyword in &prog.keyword {
        write_string_with_lang(out, "keyword", keyword);
    }
    for language in &prog.language {
        write_string_with_lang(out, "language", language);
    }
    if let Some(ref orig_language) = prog.orig_language {
        write_string_with_lang(out, "orig-language", orig_language);
    }
    if let Some(ref length) = prog.length {
        write_length(out, length);
    }
    if let Some(ref icon) = prog.icon {
        write_icon(out, icon);
    }
    for url in &prog.url {
        write_url(out, url);
    }
    for country in &prog.country {
        write_string_with_lang(out, "country", country);
    }
    for episode_num in &prog.episode_num {
        out.push_str("    <episode-num");
        if let Some(ref system) = episode_num.system {
            out.push_str(" system=\"");
            write_escaped(out, system);
            out.push('"');
        }
        out.push('>');
        write_escaped(out, &episode_num.value);
        out.push_str("</episode-num>\n");
    }
    if let Some(ref video) = prog.video {
        write_video(out, video);
    }
    for audio in &prog.audio {
        write_audio(out, audio);
    }
    if prog.is_rerun || prog.previously_shown.is_some() {
        write_previously_shown(out, prog)?;
    }
    if prog.is_premiere || prog.premiere.is_some() {
        write_optional_text_flag(out, "premiere", prog.premiere.as_ref());
    }
    if prog.is_last_chance || prog.last_chance.is_some() {
        write_optional_text_flag(out, "last-chance", prog.last_chance.as_ref());
    }
    if prog.is_new {
        out.push_str("    <new/>\n");
    }
    for subtitles in &prog.subtitles {
        write_subtitles(out, subtitles);
    }
    for rating in &prog.rating {
        write_rating(out, "rating", rating);
    }
    for rating in &prog.star_rating {
        write_rating(out, "star-rating", rating);
    }
    for review in &prog.review {
        write_review(out, review)?;
    }
    for image in &prog.image {
        out.push_str("    <image");
        if let Some(ref image_type) = image.image_type {
            out.push_str(" type=\"");
            write_escaped(out, image_type);
            out.push('"');
        }
        if let Some(ref size) = image.size {
            out.push_str(" size=\"");
            write_escaped(out, size);
            out.push('"');
        }
        if let Some(ref orient) = image.orient {
            out.push_str(" orient=\"");
            write_escaped(out, orient);
            out.push('"');
        }
        if let Some(ref system) = image.system {
            out.push_str(" system=\"");
            write_escaped(out, system);
            out.push('"');
        }
        out.push('>');
        write_escaped(out, &image.url);
        out.push_str("</image>\n");
    }

    out.push_str("  </programme>\n");
    Ok(())
}

fn validate_programme(prog: &EpgProgramme) -> Result<(), XmltvError> {
    if prog.channel.is_empty() {
        return Err(XmltvError::Validation(
            "programme channel is required for XMLTV serialization".into(),
        ));
    }
    if prog.start.is_none() {
        return Err(XmltvError::Validation(
            "programme start is required for XMLTV serialization".into(),
        ));
    }
    if prog.title.is_empty() {
        return Err(XmltvError::Validation(
            "programme title is required for XMLTV serialization".into(),
        ));
    }

    if let Some(ref pdc_start) = prog.pdc_start {
        validate_xmltv_timestamp(pdc_start)?;
    }
    if let Some(ref vps_start) = prog.vps_start {
        validate_xmltv_timestamp(vps_start)?;
    }
    if let Some(ref previously_shown) = prog.previously_shown
        && let Some(ref start) = previously_shown.start
    {
        validate_xmltv_timestamp(start)?;
    }

    for review in &prog.review {
        if review.review_type.is_none() {
            return Err(XmltvError::Validation(
                "review type is required for XMLTV serialization".into(),
            ));
        }
    }

    Ok(())
}

fn write_credits(out: &mut String, credits: &EpgCredits) {
    out.push_str("    <credits>\n");
    for person in &credits.director {
        write_credit_person(out, "director", person);
    }
    for person in &credits.actor {
        write_credit_person(out, "actor", person);
    }
    for person in &credits.writer {
        write_credit_person(out, "writer", person);
    }
    for person in &credits.adapter {
        write_credit_person(out, "adapter", person);
    }
    for person in &credits.producer {
        write_credit_person(out, "producer", person);
    }
    for person in &credits.composer {
        write_credit_person(out, "composer", person);
    }
    for person in &credits.editor {
        write_credit_person(out, "editor", person);
    }
    for person in &credits.presenter {
        write_credit_person(out, "presenter", person);
    }
    for person in &credits.commentator {
        write_credit_person(out, "commentator", person);
    }
    for person in &credits.guest {
        write_credit_person(out, "guest", person);
    }
    out.push_str("    </credits>\n");
}

fn write_credit_person(out: &mut String, tag: &str, person: &EpgPerson) {
    out.push_str("      <");
    out.push_str(tag);
    if tag == "actor" {
        if let Some(ref role) = person.role {
            out.push_str(" role=\"");
            write_escaped(out, role);
            out.push('"');
        }
        if person.guest {
            out.push_str(" guest=\"yes\"");
        }
    }
    out.push('>');

    let has_children = !person.images.is_empty() || !person.urls.is_empty();
    if !person.name.is_empty() {
        write_escaped(out, &person.name);
    }

    if has_children {
        out.push('\n');
        for image in &person.images {
            out.push_str("        <image>");
            write_escaped(out, image);
            out.push_str("</image>\n");
        }
        for url in &person.urls {
            out.push_str("        <url>");
            write_escaped(out, url);
            out.push_str("</url>\n");
        }
        out.push_str("      </");
        out.push_str(tag);
        out.push_str(">\n");
    } else {
        out.push_str("</");
        out.push_str(tag);
        out.push_str(">\n");
    }
}

fn write_string_with_lang(out: &mut String, tag: &str, swl: &EpgStringWithLang) {
    out.push_str("    <");
    out.push_str(tag);
    if let Some(ref lang) = swl.lang {
        out.push_str(" lang=\"");
        write_escaped(out, lang);
        out.push('"');
    }
    out.push('>');
    write_escaped(out, &swl.value);
    out.push_str("</");
    out.push_str(tag);
    out.push_str(">\n");
}

fn write_length(out: &mut String, length: &EpgLength) {
    out.push_str("    <length units=\"");
    out.push_str(match length.units {
        EpgLengthUnit::Seconds => "seconds",
        EpgLengthUnit::Minutes => "minutes",
        EpgLengthUnit::Hours => "hours",
    });
    out.push_str("\">");
    out.push_str(&length.value.to_string());
    out.push_str("</length>\n");
}

fn write_icon(out: &mut String, icon: &EpgIcon) {
    out.push_str("    <icon src=\"");
    write_escaped(out, &icon.src);
    out.push('"');
    if let Some(width) = icon.width {
        out.push_str(" width=\"");
        out.push_str(&width.to_string());
        out.push('"');
    }
    if let Some(height) = icon.height {
        out.push_str(" height=\"");
        out.push_str(&height.to_string());
        out.push('"');
    }
    out.push_str("/>\n");
}

fn write_url(out: &mut String, url: &EpgUrl) {
    out.push_str("    <url");
    if let Some(ref system) = url.system {
        out.push_str(" system=\"");
        write_escaped(out, system);
        out.push('"');
    }
    out.push('>');
    write_escaped(out, &url.value);
    out.push_str("</url>\n");
}

fn write_rating(out: &mut String, tag: &str, rating: &EpgRating) {
    out.push_str("    <");
    out.push_str(tag);
    if let Some(ref system) = rating.system {
        out.push_str(" system=\"");
        write_escaped(out, system);
        out.push('"');
    }
    out.push_str(">\n");
    out.push_str("      <value>");
    write_escaped(out, &rating.value);
    out.push_str("</value>\n");
    for icon in &rating.icons {
        out.push_str("      <icon src=\"");
        write_escaped(out, &icon.src);
        out.push('"');
        if let Some(width) = icon.width {
            out.push_str(" width=\"");
            out.push_str(&width.to_string());
            out.push('"');
        }
        if let Some(height) = icon.height {
            out.push_str(" height=\"");
            out.push_str(&height.to_string());
            out.push('"');
        }
        out.push_str("/>\n");
    }
    out.push_str("    </");
    out.push_str(tag);
    out.push_str(">\n");
}

fn write_video(out: &mut String, video: &EpgVideo) {
    out.push_str("    <video>\n");
    if let Some(present) = video.present {
        out.push_str("      <present>");
        out.push_str(if present { "yes" } else { "no" });
        out.push_str("</present>\n");
    }
    if let Some(colour) = video.colour {
        out.push_str("      <colour>");
        out.push_str(if colour { "yes" } else { "no" });
        out.push_str("</colour>\n");
    }
    if let Some(ref aspect) = video.aspect {
        out.push_str("      <aspect>");
        write_escaped(out, aspect);
        out.push_str("</aspect>\n");
    }
    if let Some(ref quality) = video.quality {
        out.push_str("      <quality>");
        write_escaped(out, quality);
        out.push_str("</quality>\n");
    }
    out.push_str("    </video>\n");
}

fn write_audio(out: &mut String, audio: &EpgAudio) {
    out.push_str("    <audio>\n");
    if let Some(present) = audio.present {
        out.push_str("      <present>");
        out.push_str(if present { "yes" } else { "no" });
        out.push_str("</present>\n");
    }
    if let Some(ref stereo) = audio.stereo {
        out.push_str("      <stereo>");
        write_escaped(out, stereo);
        out.push_str("</stereo>\n");
    }
    out.push_str("    </audio>\n");
}

fn write_previously_shown(out: &mut String, prog: &EpgProgramme) -> Result<(), XmltvError> {
    out.push_str("    <previously-shown");
    if let Some(ref previously_shown) = prog.previously_shown {
        if let Some(ref start) = previously_shown.start {
            out.push_str(" start=\"");
            write_escaped(out, start);
            out.push('"');
        }
        if let Some(ref channel) = previously_shown.channel {
            out.push_str(" channel=\"");
            write_escaped(out, channel);
            out.push('"');
        }
    }
    out.push_str("/>\n");
    Ok(())
}

fn write_optional_text_flag(out: &mut String, tag: &str, value: Option<&EpgStringWithLang>) {
    if let Some(value) = value {
        out.push_str("    <");
        out.push_str(tag);
        if let Some(ref lang) = value.lang {
            out.push_str(" lang=\"");
            write_escaped(out, lang);
            out.push('"');
        }
        out.push('>');
        write_escaped(out, &value.value);
        out.push_str("</");
        out.push_str(tag);
        out.push_str(">\n");
    } else {
        out.push_str("    <");
        out.push_str(tag);
        out.push_str("/>\n");
    }
}

fn write_subtitles(out: &mut String, subtitles: &EpgSubtitles) {
    out.push_str("    <subtitles");
    if let Some(subtitle_type) = subtitles.subtitle_type {
        out.push_str(" type=\"");
        out.push_str(match subtitle_type {
            crispy_iptv_types::epg::EpgSubtitleType::Teletext => "teletext",
            crispy_iptv_types::epg::EpgSubtitleType::Onscreen => "onscreen",
            crispy_iptv_types::epg::EpgSubtitleType::DeafSigned => "deaf-signed",
        });
        out.push('"');
    }

    if let Some(ref language) = subtitles.language {
        out.push_str(">\n");
        out.push_str("      <language");
        if let Some(ref lang) = language.lang {
            out.push_str(" lang=\"");
            write_escaped(out, lang);
            out.push('"');
        }
        out.push('>');
        write_escaped(out, &language.value);
        out.push_str("</language>\n");
        out.push_str("    </subtitles>\n");
    } else {
        out.push_str("/>\n");
    }
}

fn write_review(out: &mut String, review: &EpgReview) -> Result<(), XmltvError> {
    let review_type = review.review_type.as_deref().ok_or_else(|| {
        XmltvError::Validation("review type is required for XMLTV serialization".into())
    })?;

    out.push_str("    <review type=\"");
    write_escaped(out, review_type);
    out.push('"');
    if let Some(ref source) = review.source {
        out.push_str(" source=\"");
        write_escaped(out, source);
        out.push('"');
    }
    if let Some(ref reviewer) = review.reviewer {
        out.push_str(" reviewer=\"");
        write_escaped(out, reviewer);
        out.push('"');
    }
    if let Some(ref lang) = review.lang {
        out.push_str(" lang=\"");
        write_escaped(out, lang);
        out.push('"');
    }
    out.push('>');
    write_escaped(out, &review.value);
    out.push_str("</review>\n");
    Ok(())
}

/// Escape XML special characters in text content and attribute values.
fn write_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crispy_iptv_types::epg::{
        EpgLength, EpgLengthUnit, EpgPreviouslyShown, EpgSubtitleType, EpgSubtitles,
    };
    use smallvec::smallvec;

    #[test]
    fn write_empty_document() {
        let doc = XmltvDocument::default();
        let xml = write(&doc).unwrap();
        assert!(xml.contains("<?xml version=\"1.0\""));
        assert!(xml.contains("<!DOCTYPE tv"));
        assert!(xml.contains("<tv>"));
        assert!(xml.contains("</tv>"));
    }

    #[test]
    fn write_parse_roundtrip() {
        let doc = XmltvDocument {
            channels: vec![EpgChannel {
                id: "ch1".into(),
                display_name: smallvec![EpgStringWithLang::with_lang("BBC One", "en")],
                icon: Some(EpgIcon {
                    src: "https://example.com/icon.png".into(),
                    width: Some(100),
                    height: Some(50),
                }),
                url: Some(EpgUrl {
                    value: "https://example.com".into(),
                    system: None,
                }),
                ..Default::default()
            }],
            programmes: vec![EpgProgramme {
                channel: "ch1".into(),
                start: Some(1_736_942_400),
                stop: Some(1_736_946_000),
                title: smallvec![EpgStringWithLang::with_lang("Test Show", "en")],
                desc: smallvec![EpgStringWithLang::new("A description")],
                is_new: true,
                ..Default::default()
            }],
        };

        let xml = write(&doc).unwrap();
        let parsed = crate::parse(&xml).unwrap();

        assert_eq!(parsed.channels.len(), 1);
        assert_eq!(parsed.channels[0].id, "ch1");
        assert_eq!(parsed.channels[0].display_name[0].value, "BBC One");
        assert_eq!(
            parsed.channels[0].icon.as_ref().unwrap().src,
            "https://example.com/icon.png"
        );
        assert_eq!(
            parsed.channels[0]
                .url
                .as_ref()
                .map(|url| url.value.as_str()),
            Some("https://example.com")
        );

        assert_eq!(parsed.programmes.len(), 1);
        assert_eq!(parsed.programmes[0].channel, "ch1");
        assert_eq!(parsed.programmes[0].start, Some(1_736_942_400));
        assert_eq!(parsed.programmes[0].stop, Some(1_736_946_000));
        assert_eq!(parsed.programmes[0].title[0].value, "Test Show");
        assert_eq!(parsed.programmes[0].desc[0].value, "A description");
        assert!(parsed.programmes[0].is_new);
    }

    #[test]
    fn write_escapes_special_chars() {
        let doc = XmltvDocument {
            channels: vec![],
            programmes: vec![EpgProgramme {
                channel: "ch1".into(),
                start: Some(0),
                title: smallvec![EpgStringWithLang::new("Rock & Roll <Live>")],
                ..Default::default()
            }],
        };

        let xml = write(&doc).unwrap();
        assert!(xml.contains("Rock &amp; Roll &lt;Live&gt;"));
    }

    #[test]
    fn write_ratings() {
        let doc = XmltvDocument {
            channels: vec![],
            programmes: vec![EpgProgramme {
                channel: "ch1".into(),
                start: Some(0),
                title: smallvec![EpgStringWithLang::new("Rated")],
                rating: smallvec![EpgRating {
                    value: "PG-13".into(),
                    system: Some("MPAA".into()),
                    icons: smallvec![EpgIcon {
                        src: "https://example.com/rating.png".into(),
                        width: None,
                        height: None,
                    }],
                }],
                star_rating: smallvec![EpgRating {
                    value: "8/10".into(),
                    system: Some("imdb".into()),
                    icons: smallvec![],
                }],
                ..Default::default()
            }],
        };

        let xml = write(&doc).unwrap();
        assert!(xml.contains("<rating system=\"MPAA\">"));
        assert!(xml.contains("<value>PG-13</value>"));
        assert!(xml.contains("<icon src=\"https://example.com/rating.png\"/>"));
        assert!(xml.contains("<star-rating system=\"imdb\">"));
        assert!(xml.contains("<value>8/10</value>"));
    }

    #[test]
    fn write_video_audio_review() {
        let doc = XmltvDocument {
            channels: vec![],
            programmes: vec![EpgProgramme {
                channel: "ch1".into(),
                start: Some(0),
                title: smallvec![EpgStringWithLang::new("Show")],
                language: smallvec![EpgStringWithLang::with_lang("English", "en")],
                orig_language: Some(EpgStringWithLang::with_lang("French", "fr")),
                length: Some(EpgLength {
                    value: 60,
                    units: EpgLengthUnit::Minutes,
                }),
                url: smallvec![EpgUrl {
                    value: "https://example.com/programme".into(),
                    system: Some("official".into()),
                }],
                country: smallvec![EpgStringWithLang::new("GB")],
                video: Some(EpgVideo {
                    present: Some(true),
                    colour: Some(true),
                    aspect: Some("16:9".into()),
                    quality: Some("HDTV".into()),
                }),
                audio: smallvec![EpgAudio {
                    present: Some(true),
                    stereo: Some("surround".into()),
                }],
                review: smallvec![EpgReview {
                    value: "Great show".into(),
                    review_type: Some("text".into()),
                    source: Some("NYT".into()),
                    reviewer: Some("Jane".into()),
                    lang: Some("en".into()),
                }],
                previously_shown: Some(EpgPreviouslyShown {
                    start: Some("20240115120000 +0000".into()),
                    channel: Some("archive".into()),
                }),
                is_rerun: true,
                keyword: smallvec![EpgStringWithLang::with_lang("Drama", "en")],
                subtitles: smallvec![EpgSubtitles {
                    subtitle_type: Some(EpgSubtitleType::Onscreen),
                    language: Some(EpgStringWithLang::with_lang("English", "en")),
                }],
                ..Default::default()
            }],
        };

        let xml = write(&doc).unwrap();

        assert!(xml.contains("<language lang=\"en\">English</language>"));
        assert!(xml.contains("<orig-language lang=\"fr\">French</orig-language>"));
        assert!(xml.contains("<length units=\"minutes\">60</length>"));
        assert!(xml.contains("<url system=\"official\">https://example.com/programme</url>"));
        assert!(xml.contains("<country>GB</country>"));
        assert!(xml.contains("<video>"));
        assert!(xml.contains("<audio>"));
        assert!(
            xml.contains("<previously-shown start=\"20240115120000 +0000\" channel=\"archive\"/>")
        );
        assert!(xml.contains("<subtitles type=\"onscreen\">"));
        assert!(xml.contains("<review type=\"text\""));

        let parsed = crate::parse(&xml).unwrap();
        let prog = &parsed.programmes[0];
        assert_eq!(prog.language[0].value, "English");
        assert_eq!(prog.length.as_ref().unwrap().value, 60);
        assert_eq!(prog.length.as_ref().unwrap().units, EpgLengthUnit::Minutes);
        assert_eq!(prog.url[0].system.as_deref(), Some("official"));
        assert_eq!(prog.country[0].value, "GB");
        assert!(prog.previously_shown.is_some());
        assert_eq!(
            prog.subtitles[0].subtitle_type,
            Some(EpgSubtitleType::Onscreen)
        );
    }

    #[test]
    fn write_channel_multiple_icons_and_urls() {
        let doc = XmltvDocument {
            channels: vec![EpgChannel {
                id: "ch1".into(),
                display_name: smallvec![EpgStringWithLang::new("Channel")],
                icon: None,
                url: None,
                icons: smallvec![
                    EpgIcon {
                        src: "https://example.com/a.png".into(),
                        width: Some(100),
                        height: None,
                    },
                    EpgIcon {
                        src: "https://example.com/b.png".into(),
                        width: None,
                        height: None,
                    },
                ],
                urls: smallvec![
                    EpgUrl {
                        value: "https://example.com".into(),
                        system: None,
                    },
                    EpgUrl {
                        value: "https://mirror.example.com".into(),
                        system: None,
                    },
                ],
            }],
            programmes: vec![],
        };

        let xml = write(&doc).unwrap();
        assert!(xml.contains("icon src=\"https://example.com/a.png\""));
        assert!(xml.contains("icon src=\"https://example.com/b.png\""));
        assert!(xml.contains("<url>https://example.com</url>"));
        assert!(xml.contains("<url>https://mirror.example.com</url>"));

        let parsed = crate::parse(&xml).unwrap();
        let channel = &parsed.channels[0];
        assert_eq!(channel.icons.len(), 2);
        assert_eq!(channel.urls.len(), 2);
    }

    #[test]
    fn write_emits_dtd_programme_order_and_length_units() {
        let programme = EpgProgramme {
            channel: "ch1".into(),
            start: Some(1_736_942_400),
            title: smallvec![EpgStringWithLang::new("Title")],
            sub_title: smallvec![EpgStringWithLang::new("Subtitle")],
            desc: smallvec![EpgStringWithLang::new("Description")],
            credits: Some(EpgCredits {
                director: smallvec![EpgPerson {
                    name: "Director".into(),
                    ..Default::default()
                }],
                ..Default::default()
            }),
            date: Some("2025".into()),
            category: smallvec![EpgStringWithLang::new("Drama")],
            keyword: smallvec![EpgStringWithLang::new("Featured")],
            language: smallvec![EpgStringWithLang::new("English")],
            orig_language: Some(EpgStringWithLang::new("French")),
            length: Some(EpgLength {
                value: 60,
                units: EpgLengthUnit::Minutes,
            }),
            icon: Some(EpgIcon {
                src: "https://example.com/icon.png".into(),
                width: None,
                height: None,
            }),
            url: smallvec![EpgUrl {
                value: "https://example.com/programme".into(),
                system: None,
            }],
            country: smallvec![EpgStringWithLang::new("GB")],
            episode_num: smallvec![crispy_iptv_types::epg::EpgEpisodeNumber {
                value: "0.1.2".into(),
                system: Some("xmltv_ns".into()),
            }],
            video: Some(EpgVideo {
                present: Some(true),
                colour: None,
                aspect: None,
                quality: None,
            }),
            audio: smallvec![EpgAudio {
                present: Some(true),
                stereo: Some("stereo".into()),
            }],
            previously_shown: Some(EpgPreviouslyShown {
                start: Some("20240101000000 +0000".into()),
                channel: Some("archive".into()),
            }),
            is_rerun: true,
            premiere: Some(EpgStringWithLang::new("Broadcast premiere")),
            is_premiere: true,
            last_chance: Some(EpgStringWithLang::new("Final airing")),
            is_last_chance: true,
            is_new: true,
            subtitles: smallvec![EpgSubtitles {
                subtitle_type: Some(EpgSubtitleType::Onscreen),
                language: Some(EpgStringWithLang::new("English")),
            }],
            rating: smallvec![EpgRating {
                value: "PG".into(),
                system: Some("MPAA".into()),
                icons: smallvec![],
            }],
            star_rating: smallvec![EpgRating {
                value: "4/5".into(),
                system: Some("imdb".into()),
                icons: smallvec![],
            }],
            review: smallvec![EpgReview {
                value: "Great".into(),
                review_type: Some("text".into()),
                source: None,
                reviewer: None,
                lang: None,
            }],
            image: smallvec![crispy_iptv_types::epg::EpgImage {
                url: "https://example.com/poster.jpg".into(),
                image_type: Some("poster".into()),
                size: Some("1".into()),
                orient: Some("P".into()),
                system: Some("tmdb".into()),
            }],
            ..Default::default()
        };
        let doc = XmltvDocument {
            channels: vec![],
            programmes: vec![programme],
        };

        let xml = write(&doc).unwrap();
        assert!(xml.contains("<length units=\"minutes\">60</length>"));

        let order = [
            "<title",
            "<sub-title",
            "<desc",
            "<credits>",
            "<date>",
            "<category",
            "<keyword",
            "<language",
            "<orig-language",
            "<length",
            "<icon",
            "<url",
            "<country",
            "<episode-num",
            "<video>",
            "<audio>",
            "<previously-shown",
            "<premiere",
            "<last-chance",
            "<new/>",
            "<subtitles",
            "<rating",
            "<star-rating",
            "<review",
            "<image",
        ];

        let mut last_index = 0;
        for needle in order {
            let index = xml.find(needle).unwrap();
            assert!(index >= last_index, "{needle} is out of order");
            last_index = index;
        }
    }

    #[test]
    fn write_rejects_invalid_programmes() {
        let missing_start = XmltvDocument {
            channels: vec![],
            programmes: vec![EpgProgramme {
                channel: "ch1".into(),
                title: smallvec![EpgStringWithLang::new("Show")],
                ..Default::default()
            }],
        };
        let err = write(&missing_start).unwrap_err();
        assert!(matches!(err, XmltvError::Validation(_)));

        let missing_title = XmltvDocument {
            channels: vec![],
            programmes: vec![EpgProgramme {
                channel: "ch1".into(),
                start: Some(0),
                ..Default::default()
            }],
        };
        let err = write(&missing_title).unwrap_err();
        assert!(matches!(err, XmltvError::Validation(_)));
    }

    #[test]
    fn write_rejects_out_of_range_timestamp() {
        let doc = XmltvDocument {
            channels: vec![],
            programmes: vec![EpgProgramme {
                channel: "ch1".into(),
                start: Some(i64::MAX),
                title: smallvec![EpgStringWithLang::new("Show")],
                ..Default::default()
            }],
        };

        let err = write(&doc).unwrap_err();
        assert!(matches!(err, XmltvError::Timestamp(_)));
    }
}
