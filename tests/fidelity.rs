use crispy_xmltv::types::{XmltvLengthUnit, XmltvSubtitleType};
use crispy_xmltv::{parse, write};

const RICH_XMLTV: &str = include_str!("fixtures/rich_roundtrip.xml");

#[test]
fn rich_fixture_roundtrips_supported_xmltv_fields() {
    let parsed = parse(RICH_XMLTV).expect("fixture parses");

    assert_eq!(parsed.channels.len(), 1);
    let channel = &parsed.channels[0];
    assert_eq!(channel.id, "ch.rich");
    assert_eq!(channel.display_name.len(), 2);
    assert_eq!(channel.urls.len(), 2);
    assert_eq!(channel.urls[0].system.as_deref(), Some("official"));
    assert_eq!(channel.urls[1].system.as_deref(), Some("mirror"));
    assert_eq!(
        channel.url.as_ref().and_then(|url| url.system.as_deref()),
        Some("official")
    );

    assert_eq!(parsed.programmes.len(), 1);
    let programme = &parsed.programmes[0];
    assert_eq!(programme.title[0].value, "Feature Show");
    assert_eq!(programme.sub_title[0].value, "Episode Subtitle");
    assert_eq!(programme.language[0].value, "English");
    assert_eq!(programme.length.as_ref().unwrap().value, 60);
    assert_eq!(
        programme.length.as_ref().unwrap().units,
        XmltvLengthUnit::Minutes
    );
    assert_eq!(programme.url[0].system.as_deref(), Some("official"));
    assert_eq!(programme.country[0].value, "GB");
    assert_eq!(programme.image.len(), 1);
    assert_eq!(programme.image[0].system.as_deref(), Some("tmdb"));
    assert_eq!(programme.episode_num.len(), 2);
    assert_eq!(programme.episode_num[0].system.as_deref(), Some("xmltv_ns"));
    assert_eq!(programme.rating[0].system.as_deref(), Some("MPAA"));
    assert_eq!(programme.review[0].review_type, crispy_iptv_types::epg::EpgReviewType::Text);
    assert_eq!(
        programme
            .previously_shown
            .as_ref()
            .and_then(|shown| shown.channel.as_deref()),
        Some("archive")
    );
    assert_eq!(
        programme.subtitles[0].subtitle_type,
        Some(XmltvSubtitleType::Onscreen)
    );
    assert!(programme.is_new);
    assert!(programme.is_premiere);

    let written = write(&parsed).expect("fixture serializes");
    let reparsed = parse(&written).expect("roundtrip parses");
    let reparsed_channel = &reparsed.channels[0];
    let reparsed_programme = &reparsed.programmes[0];

    assert_eq!(reparsed_channel.urls.len(), 2);
    assert_eq!(reparsed_channel.urls[0].system.as_deref(), Some("official"));
    assert_eq!(reparsed_channel.urls[1].system.as_deref(), Some("mirror"));
    assert_eq!(reparsed_programme.language[0].value, "English");
    assert_eq!(
        reparsed_programme.length.as_ref().unwrap().units,
        XmltvLengthUnit::Minutes
    );
    assert_eq!(
        reparsed_programme.url[0].system.as_deref(),
        Some("official")
    );
    assert_eq!(reparsed_programme.country[0].value, "GB");
    assert_eq!(reparsed_programme.image[0].system.as_deref(), Some("tmdb"));
    assert_eq!(
        reparsed_programme.episode_num[1].system.as_deref(),
        Some("onscreen")
    );
    assert_eq!(
        reparsed_programme.review[0].source.as_deref(),
        Some("Example Review")
    );
    assert_eq!(
        reparsed_programme.subtitles[0].subtitle_type,
        Some(XmltvSubtitleType::Onscreen)
    );

    let programme_xml = &written[written
        .find("<programme")
        .expect("programme element present")..];
    let order = [
        "<title",
        "<sub-title",
        "<desc",
        "<category",
        "<keyword",
        "<language",
        "<orig-language",
        "<length",
        "<url",
        "<country",
        "<episode-num",
        "<previously-shown",
        "<subtitles",
        "<rating",
        "<star-rating",
        "<review",
        "<image",
    ];
    let mut last_index = 0;
    for needle in order {
        let index = programme_xml
            .find(needle)
            .expect("tag present in serialized rich fixture");
        assert!(index >= last_index, "{needle} is out of order");
        last_index = index;
    }
}
