# crispy-xmltv

Streaming XMLTV parser and writer for large EPG inputs.

## Status

Extracted from CrispyTivi. Intended as a reusable Rust crate for XMLTV ingestion, transformation, and serialization.

## What This Crate Provides

- event-driven XMLTV parsing
- support for large files without DOM-style full buffering
- typed programme/channel output
- XMLTV writing support
- compressed input helpers for common archive formats

## Installation

```toml
[dependencies]
crispy-xmltv = "0.1"
```

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

let out = write(&doc);
assert!(out.contains("channel"));
```

## Primary Use Cases

- EPG ingestion pipelines
- guide normalization/transformation
- IPTV app backends
- EPG archival tools

## Relationship To Other Crates

- uses `crispy-iptv-types` EPG models
- can feed app-specific mapping layers such as those in CrispyTivi

## Non-Goals

- network fetching
- provider-specific scheduling logic
- persistence

## Caveats

- public docs should clearly state supported XMLTV variations and compression formats before release
