# crispy-xmltv

Streaming XMLTV parser and writer for large EPG inputs.

## What This Crate Is

`crispy-xmltv` is an event-driven XMLTV reader/writer intended for guide import pipelines and IPTV applications. It favors streaming-style parsing over DOM-style full buffering so large guide files remain practical.

## What It Provides

- `parse(&str)`
- `parse_reader(...)`
- `parse_compressed(...)`
- `write(&XmltvDocument) -> Result<String, XmltvError>`
- automatic decompression helpers for common compressed XMLTV inputs
- episode-number parsing helpers

## Installation

```toml
[dependencies]
crispy-xmltv = "0.1.1"
```

MSRV: Rust `1.85`

## Quick Start

```rust
use crispy_xmltv::{parse, write};

let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<tv>
  <channel id="ch1"><display-name>Channel One</display-name></channel>
  <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
    <title>Test Show</title>
  </programme>
</tv>"#;

let doc = parse(xml).unwrap();
assert_eq!(doc.channels.len(), 1);
assert_eq!(doc.programmes.len(), 1);

let output = write(&doc).unwrap();
assert!(output.contains("programme"));
```

## Main Types

- `XmltvDocument`
- `XmltvError`
- shared EPG types re-exported through `types`

## Coverage

The crate is aimed at a practical, explicitly documented XMLTV subset.

Supported and round-trippable fields include:
- channel `display-name`, `icon`, and multiple `url` elements with optional `system`
- programme titles, subtitles, descriptions, categories, dates, explicit lengths, credits
- programme `language`, `orig-language`, `url`, `country`, `previously-shown`, and `subtitles`
- episode numbers with `system`
- `image` metadata including `type`, `size`, `orient`, and `system`
- `rating`, `star-rating`, `review`, `video`, `audio`, `keyword`, and boolean flags

Intentional omissions or non-goals:
- root `<tv>` metadata attributes are not modeled
- writing preserves structured meaning, not byte-for-byte source formatting

Writer/parser validation notes:
- programme parsing rejects malformed required `start` / `channel` attributes and invalid XMLTV timestamps, while accepting the XMLTV upstream `Date::Manip` short-zone table for named suffixes such as `BST`, `HKT`, `GMT+10`, and military-zone letters; full IANA names like `Europe/Berlin` remain intentionally unsupported
- writer serialization is fallible and rejects invalid programmes, blank channel ids, and empty or invalid reviews instead of emitting malformed XMLTV
- `<length>` values preserve explicit XMLTV units; legacy unit-less lengths are normalized to minutes on parse

## Typical Uses

- loading guide data from XMLTV feeds
- transforming XMLTV into internal application models
- exporting or cleaning guide documents
- working with compressed `.gz` / `.xz` guide sources

## Related Crates

- `crispy-iptv-types` for shared EPG models

## Current Limitations

- network fetching is intentionally out of scope
- provider-specific merge/match policy is out of scope
- caller is responsible for persistence and source refresh strategy

## License

See `LICENSE.md` and `NOTICE.md`.
