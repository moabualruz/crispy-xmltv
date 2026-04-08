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
    assert_eq!(programme.image.len(), 1);
    assert_eq!(programme.image[0].system.as_deref(), Some("tmdb"));
    assert_eq!(programme.episode_num.len(), 2);
    assert_eq!(programme.episode_num[0].system.as_deref(), Some("xmltv_ns"));
    assert_eq!(programme.rating[0].system.as_deref(), Some("MPAA"));
    assert_eq!(programme.review[0].review_type.as_deref(), Some("text"));
    assert!(programme.is_new);
    assert!(programme.is_premiere);

    let written = write(&parsed);
    let reparsed = parse(&written).expect("roundtrip parses");
    let reparsed_channel = &reparsed.channels[0];
    let reparsed_programme = &reparsed.programmes[0];

    assert_eq!(reparsed_channel.urls.len(), 2);
    assert_eq!(reparsed_channel.urls[0].system.as_deref(), Some("official"));
    assert_eq!(reparsed_channel.urls[1].system.as_deref(), Some("mirror"));
    assert_eq!(reparsed_programme.image[0].system.as_deref(), Some("tmdb"));
    assert_eq!(
        reparsed_programme.episode_num[1].system.as_deref(),
        Some("onscreen")
    );
    assert_eq!(
        reparsed_programme.review[0].source.as_deref(),
        Some("Example Review")
    );
}
