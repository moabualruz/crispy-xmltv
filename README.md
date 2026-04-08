# crispy-xmltv

Streaming XMLTV parser and writer for large EPG inputs.

## What This Crate Is

`crispy-xmltv` is an event-driven XMLTV reader/writer intended for guide import pipelines and IPTV applications. It favors streaming-style parsing over DOM-style full buffering so large guide files remain practical.

## What It Provides

- `parse(&str)`
- `parse_reader(...)`
- `parse_compressed(...)`
- `write(&XmltvDocument)`
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

let output = write(&doc);
assert!(output.contains("programme"));
```

## Main Types

- `XmltvDocument`
- `XmltvError`
- shared EPG types re-exported through `types`

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
