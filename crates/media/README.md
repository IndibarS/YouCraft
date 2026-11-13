# filmcraft-media

Media sources, still-image and image-sequence import, generated media, audio streams, and decoded
frame caching for FilmCraft.

## Image-sequence filenames

`sequence::Numbered` identifies the last run of 1–18 ASCII frame-number digits in a filename's
stem when the extension is a recognized still-image format. `/` and `\` separate directory
components. A prefix may contain other UTF-8 characters, including accented letters, CJK and emoji:
`é0001.png`, `图0001.png` and `🎞️0001.png`. Parsing preserves the prefix, number padding and suffix
when generating sibling-frame paths. Sibling recognition compares the prefix's UTF-8 bytes exactly.
Stems without ASCII digits or with a final frame-number run longer than 18 characters are not numbered
image-sequence filenames.

## Embedded video artwork

`artwork::embedded` reads MP4/MOV `moov/udta/meta/ilst/covr/data` JPEG/PNG artwork
and Matroska named cover attachments without decoding video. It uses random-access reads,
skips media payloads, caps artwork at 16 MiB, limits nesting and element counts, and returns
errors for malformed metadata. Unknown-sized Matroska clusters cannot be skipped; artwork
beyond one is currently unavailable. It does not treat arbitrary attachments as cover art.

Specifications: Matroska RFC 9559 (October 2024), §21.1 cover art and attachment elements;
QuickTime File Format metadata atom layout (Apple's living specification, consulted 2026-11-12),
with the iTunes `covr` convention (data types 13 JPEG / 14 PNG). No decoder implementation
source was used. Synthetic extraction, truncation, bit-flip, excessive-depth and size tests run
with `cargo test -p filmcraft-media --lib artwork`.
