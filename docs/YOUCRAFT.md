# YouCraft validation history

Entries record checks at the stated date; later entries supersede earlier behavior and counts.
For current behavior and source layout, see the [user guide](../README.md).

## Initial validation — 2026-10-31

- Native Linux development build produced with Rust 1.95.
- Five focused regression tests cover library UI at 900/1600 px, frame-exact seeking without project edits, search and audio gain, subtitle parsing/error handling, and saved-library round trips.
- Native virtual-display QA exercised launch, generated thumbnails, playback, pause, queue, favorites, next and search through the authenticated control channel. Library and watch screenshots were reviewed; thumbnail layout and seek-bar width were corrected.
- `cargo fmt --all --check`, `cargo clippy -p filmcraft-ui-egui --lib -- -D warnings`, `cargo xtask layers` and `cargo xtask assets` pass. The asset check covers 61 attributed assets.
- Full upstream workspace test/wasm gates were not run. No upstream commit, PR, publication or official Launcher registration was performed.

The included executable is a Linux development build, not a signed release. Physical speaker output and Windows/macOS runtimes were not verified. The viewing preferences companion is saved explicitly with the library; subtitle files are loaded per viewing session. Original video files are never rewritten by playback.

## Local library and playback improvements

Normal launches discover supported video files under Videos, Movies, Downloads, Desktop and Documents,
including Linux XDG folder overrides. Discovery runs on a background worker, skips hidden folders,
symlinks and dependency/build folders, and is bounded to 12 directory levels, 100,000 entries and
10,000 video candidates. Unsupported files are skipped. New files are picked up on the next launch;
Open videos remains available for other locations. Explicit `--demo` and `--empty` launches skip discovery.

The window close button exits normally. The library, likes, dislikes and viewing preferences are
saved automatically to the application data directory (`library.fcproj` plus its preferences companion)
and restored next launch. Explicit demo/empty sessions do not replace the automatic library.

Source playback uses the existing GPU compositor for supported color transforms, with the original
CPU color-managed fallback. Software rendering remains supported. The library renders only visible
rows, background imports pause during playback, and seeking commits once when the slider is released.
Performance still depends on codec, resolution, storage, and graphics/audio drivers.

## AV1 playback backend

Native builds use rav1d through the existing `VideoDecoder` interface. The dependency is pinned to
`2e7216c20ac2a75de204d6c8d7268b75a45959b4` (1.1.1 API) with optimized assembly and 8/10/12-bit
support. Assembly builds need NASM on x86; the installed executable needs no separate rav1d
installation. The adapter uses safe Rust, retains source timestamps and color metadata, handles
decoder backpressure and delayed output, and resets decoder state on seeks. Input packets and
picture sizes are bounded. WASM retains the original decoder.

This is a user-authorized exception to the upstream independent-codec policy. rav1d and its
upstream notices are in `docs/licenses/rav1d.txt`; the safe API's MIT notice is alongside it.

Regression fixtures can be generated with FFmpeg/libaom (test tooling only): 24 frames at 96×64,
24 fps, keyframe interval 12, IVF output, in yuv420p, yuv420p10le, yuv420p12le and gray. Decode each to matching raw
YUV as an oracle. With `eight.ivf`, `eight.yuv`, `ten.ivf`, `ten.yuv`, `twelve.ivf`, `twelve.yuv`, `mono.ivf`, and `mono.yuv` in a directory, run:

```sh
YOUCRAFT_AV1_FIXTURES=/absolute/fixture/directory cargo +1.95.0 test -p filmcraft-codecs --test rav1d -- --ignored
```

The test checks exact decoded pixels, delayed timestamps, end-of-stream drain, reset/seeking and
recovery after truncated input across all bit depths and monochrome video.

## Player controls — November 2026

The player uses overlay controls, a related-video sidebar, likes/dislikes and a floating miniplayer.
Queue and Next controls are removed; autoplay follows library order and stops at the last video.
The existing ArtCraft family code-drawn Captions/Rectangle/Proxy icons cover tracks/theater/miniplayer;
thumb icons are original code drawings under the repository licence.

Grid and hover thumbnails use a separate single-worker FrameServer, independent decoder pool and
64 MB image cache. They cannot seek or cancel the playback decoder. Grid images use a frame 10%
into the video, capped at 30 seconds. Unsupported files show a preview-unavailable label and a reason.
Hover targets use a bounded storyboard of up to 128 frames. Frames are prepared in the background and saved in a reusable disk cache; already prepared frames have no hover delay.

Audio switching uses the existing MediaSource::audio_stream path and retains the playhead.
MKV SubRip, ASS/SSA (plain text), and WebVTT tracks are loaded on a guarded background worker with
100,000-cue / 16 MB limits. External UTF-8 SRT and WebVTT remain available. Bitmap subtitles and
embedded subtitles in other containers are not supported by this implementation.

YouTube was inspected in normal, theater and open-settings states using rendered DOM, computed
styles and screenshots. Its player buttons measured 48×40 px; its overlay progress bar, compact
settings menu and right-side video list informed the layout. No YouTube scripts, artwork or icons
are bundled; this remains a native Rust app.

Validation on 2026-11-06: 11 player regression tests and the generated two-audio/two-subtitle MKV
fixture test passed. Native virtual-display checks exercised related videos, likes/dislikes, idle
controls, hover previews, skip feedback, miniplayer/Back, keyboard shortcuts and track switching.
Frankenstein exposed 2 audio and 17 text subtitle tracks; selecting its first subtitle track loaded
89 cues. Formatting, UI-library Clippy, dependency layers and attributed assets checks passed.
Physical speaker output and a full movie-length viewing session were not verified.

## Proportions and persistent previews — 2026-11-07

The player sizes its picture to the source display aspect, including pixel aspect ratio. Fullscreen
preserves the complete image; black bars encoded into the video or required by the screen's shape
are not cropped away. Grid and sidebar thumbnails use a centered aspect-preserving crop.
Fullscreen, theater and miniplayer controls preserve each other's state. Theater playback uses
full width with a scrollable video grid below it.

Preview preparation runs on the isolated thumbnail worker, with at most one waiting preparation
job. The worker prepares up to 128 evenly spaced frames, filling the beginning, middle and quarters
first. Hover and visible-grid jobs have priority. A 64 MB memory cache and 512 disk slots (maximum
512 KiB per slot, 256 MiB total) bound storage. Slots validate source metadata, file size, first/last
bytes, frame and output width; hash collisions and malformed PNGs are cache misses. Files are
written atomically through host services in the application data directory's previews-v2 folder.

Failed grid-frame decoding retries earlier positions; failed renders no longer appear as stretched
one-pixel black thumbnails. Unsupported codecs still cannot produce real decoded thumbnails.

Validation: all 13 player tests and the persistent-cache test pass. Native checks confirmed 4:3
picture sizing, theater-grid placement, and independent F/T/I preferences. Restarting the test
library reused its saved previews (9 disk-cache hits). Clippy, formatting, dependency layering and
asset attribution checks passed.

## Desktop and embedded thumbnails — 2026-11-12

Library cards reuse fresh Freedesktop PNG thumbnails on Linux before extracting MP4/MOV `covr`
or named Matroska cover attachments, then fall back to decoded frames. Extraction, PNG/JPEG
loading, and cache lookup use Rust code; no external thumbnailer or FFmpeg runtime is invoked.
Posters have separate in-memory identities from timestamped previews. Generated posters preserve
pixel aspect ratio, embedded/desktop artwork uses its own aspect, and unreadable video slates are
not saved as successful previews. The persistent cache identity version invalidates old slates.

The local library inspection found 78 matching, fresh desktop thumbnails and 82 MPEG-4 Part 2
entries (many generated development fixtures). The latter remain unsupported for playback;
errors now identify `mp4v` as MPEG-4 Part 2 rather than only “unknown.” No new video decoder was
added. Existing H.264/HEVC/VP8/VP9/AV1 and other decoder coverage is unchanged.

Validation: 24 UI/player/poster/cache tests, two artwork parser tests (including truncation and
bit-flip mutations under catch_unwind), and the codec-label test pass. Focused library Clippy,
formatting, layers and asset attribution checks pass. Native virtual-display QA using an isolated
copy of the real library confirmed that `output_black_bg.mp4` changes from “Preview unavailable”
to its existing desktop thumbnail. The original library and media files were not modified.
Full workspace CI and Windows/macOS thumbnail-cache integration were not tested.

## Player identity and unused-code cleanup — 2026-11-12

Removed the disabled upstream update runtime, its polling/dialogs, Windows update-helper entry,
and the craft-update dependency. The update command remains a local source-build explanation.
Removed the write-only discovery completion flag and the wordmark's unused theme argument and
always-present Option wrapper. Internal engine/crate names, control environment variables,
project-format identifiers, and upstream licensing/contributor credits remain compatible.

The native CLI, startup errors, library file filters, player About/recovery/error windows, and
player/native menu labels identify YouCraft. Native logs now use youcraft.log, and the default
filter includes the youcraft binary's messages alongside the shared engine crates. Windows
resource descriptions identify the video player while retaining the original contributor credit.
Upstream About links are explicitly labeled as upstream rather than as this fork's source/support.

Cleanup validation: 90 native tests and 25 player/poster/cache tests pass. Native socket tests
were run outside the network-restricted sandbox. Focused Clippy, formatting, layers, and assets
checks pass. The About window was visually inspected on an isolated virtual display; `--version`
reports `youcraft 0.14.0`, and startup writes `Logs/youcraft.log`. The local launcher binary was
rebuilt. Full workspace CI and native macOS/Windows behavior were not validated in this pass.

## Source organization — 2026-11-13

Split library persistence/discovery, track selection, and miniplayer behavior into private player
submodules, preserving the public API. Scoped desktop-thumbnail hashing to Linux and documented
the module boundaries. The existing ArtCraft rustfmt settings remain unchanged.

Validation: 28 player/poster/cache/search tests pass, plus the miniplayer rendering check.
Focused native/UI/media Clippy, workspace formatting, dependency layers, asset attribution,
and native build passed. The rendered miniplayer was inspected and the local launcher binary
updated. Full workspace CI and other native platforms were not checked in this pass.

## Linux packaging — 2026-11-14

Adapted the inherited Crafting Apps pipeline to YouCraft: one staged tree for nFPM DEB/RPM,
AppImage and tarball outputs, with the Flatpak bundle consuming the tarball. Package executables
are named `youcraft` and `youcraft-cli`; Cargo package names remain unchanged. Desktop metadata
matches `local.youcraft.player`, notices include rav1d and bundled fonts, and AppImage updates
require an explicit fork repository. DEB/RPM glibc dependencies are measured from the binaries.

The optimized release build passed and produced all four Linux artifacts in `dist/release`.
Package metadata and required license files were inspected. The extracted DEB executable,
tarball installation under a temporary path containing spaces, and AppImage all passed version
smoke checks. Desktop metadata validation, shell syntax, invalid-argument checks, dependency
layers, assets, and local documentation links passed. No system-wide installation was performed.

These x86_64 packages require glibc 2.43. The installed Flatpak 25.08 runtime provides 2.42, so a
compatible older-host build is needed; no Flatpak artifact was validated. Windows/macOS and full
workspace CI were not tested. See the [packaging guide](../packaging/README.md) for commands.
