//! Streaming XMLTV/EPG parser and writer.
//!
//! `crispy-xmltv` targets the practical XMLTV subset needed by IPTV guide
//! pipelines while preserving the structured fields it models across
//! parse/write/parse round-trips.
//!
//! Supported coverage includes channel display names, icons, multiple URLs with
//! optional `system` attributes, and the major programme metadata fields such as
//! titles, descriptions, categories, credits, ratings, reviews, episode
//! numbers, images, programme languages, countries, subtitles, and media flags.
//!
//! Intentional omissions are documented rather than hidden:
//! - root `<tv>` metadata attributes are not modeled
//! - exact byte-for-byte source fidelity is not guaranteed
//! - serialization validates required programme fields instead of silently
//!   emitting malformed XMLTV
//!
//! # Usage
//!
//! ```rust
//! use crispy_xmltv::{parse, write};
//!
//! let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
//! <tv>
//!   <channel id="ch1">
//!     <display-name>Channel One</display-name>
//!   </channel>
//!   <programme start="20250115120000 +0000" stop="20250115130000 +0000" channel="ch1">
//!     <title>Test Show</title>
//!   </programme>
//! </tv>"#;
//!
//! let doc = parse(xml).unwrap();
//! assert_eq!(doc.channels.len(), 1);
//! assert_eq!(doc.programmes.len(), 1);
//!
//! let output = write(&doc).unwrap();
//! assert!(output.contains("<channel id=\"ch1\">"));
//! ```

pub mod compression;
pub mod episode;
pub mod error;
pub mod parser;
pub mod timestamp;
pub mod types;
pub mod writer;

pub use compression::decompress_auto;
pub use episode::{EpisodeInfo, parse_episode_number};
pub use error::XmltvError;
pub use parser::{parse, parse_compressed, parse_reader};
pub use types::XmltvDocument;
pub use writer::write;
