# YouCraft

A native, local video player with a YouTube-inspired library and watch interface. Based on FilmCraft by the ArtCraft team; this independent fork is not an official ArtCraft release.

## Start

```sh
./run-youcraft.sh
./run-youcraft.sh /absolute/path/to/video.mp4
./run-youcraft.sh --demo
```

The launcher uses `bin/youcraft` when present. To build from source, install Rust 1.95+ and the Linux development dependencies in [setup](docs/contributing.md), then run:

```sh
cargo +1.95.0 run -p filmcraft --bin youcraft
```

For DEB, RPM, AppImage, tarball, and Flatpak builds, see the [packaging guide](packaging/README.md).

## Watching videos

- **Open videos** or drop local files into the window. Click a thumbnail to watch.
- **Space / K** plays or pauses; **J / L** skips 10 seconds; arrows skip 5 seconds.
- **M** mutes; **F** or a double click enters fullscreen; Escape exits.
- Use the seek bar, volume slider, Theater view, and Loop control while watching.
- The right sidebar lists your videos. **Autoplay** continues with the following library video; there is no queue.
- Use the **CC icon** to choose an audio track, embedded MKV text subtitle track, or external SRT/WebVTT file. ASS styling and bitmap subtitles are not rendered.
- Search the local library or revisit **History** and **Liked videos**.
- **Library → Save library** saves the media references as `.fcproj` and likes, dislikes, history and volume in a `.fcproj.youcraft.json` companion. Keep both files together and reopen with **Library → Open library**. Original media stays in its original location.

## Exact architectural foundation

Upstream: `https://github.com/storytold/filmcraft`, commit `fed0f42fd52012a337d744743a93409c0b1e8f9d`.

The Cargo workspace, dependency layering, Rust codecs and containers, exact integer timebase, headless engine, media cache, rendering workers, source playback clock, native audio output, command registry and automation protocol come directly from the Crafting Apps code. This is a frontend specialization, not a web wrapper or a reimplementation of video decoding.

`crates/ui-egui/src/player.rs` owns the player shell and serializable viewing state. The source-only picture path in `panels/monitor.rs` reuses the asynchronous frame renderer. Source audio applies a shared atomic volume control. `apps/filmcraft` provides the OS integration and builds the `youcraft` executable. The original editor frontend remains as library code for upstream compatibility; YouCraft launches the player shell and player keyboard controls.

See the [documentation index](docs/README.md) for development guides and validation history.

## Source organization and formatting

The existing ArtCraft workspace layers and `rustfmt.toml` remain authoritative. Format with
`cargo fmt --all`; check with `cargo fmt --all --check` and `cargo xtask layers`.
The formatter uses 160-column lines and `use_small_heuristics = "Max"`.

| Location | Responsibility |
| --- | --- |
| `apps/filmcraft/src/` | Native startup, OS integration, audio, and background file discovery |
| `crates/ui-egui/src/player.rs` | Player state, shell, watch view, and playback controls |
| `crates/ui-egui/src/player/library.rs` | Library persistence, missing-file cleanup, and discovered-file imports |
| `crates/ui-egui/src/player/tracks.rs` | Audio-track choices and subtitle loading |
| `crates/ui-egui/src/player/miniplayer.rs` | Floating player layout, dragging, and resizing |
| `crates/ui-egui/src/poster.rs` | Desktop thumbnail lookup and bounded poster image loading |
| `crates/ui-egui/src/thumbnail_cache.rs` | Persistent generated-preview cache |
| `crates/media/src/artwork.rs` | Bounded embedded-artwork parsing, independent of the UI and video decoder |
| `crates/codecs/` | Video/audio decoder adapters and container-backed media sources |

Player submodules are private implementation details. Public player entry points remain available
through `filmcraft_ui_egui::player`; project changes continue through engine commands.
Regression tests live in each crate's `tests/` directory, with parser robustness tests beside the parser.

## Automation

Start with `--control PORT` and a fresh 64-hex-character `FILMCRAFT_CONTROL_TOKEN` in the environment. See [control protocol](docs/control-protocol.md). Every player control has a `player.*` automation ID. `ui.inspect` exposes viewing state; `ui.set` accepts a validated `player` patch. Native engine import, source open/seek and project save commands remain available. The inherited headless CLI/MCP package is `filmcraft-cli`.

## Scope and limitations

Local-file playback only: no YouTube streaming or account integration. Playback is normal speed. External SRT/WebVTT and embedded MKV text subtitles are supported; bitmap subtitles and ASS styling are not. Normal launches save and restore the library automatically, with explicit save/open available too. The inherited decoders are alpha software, so compatibility varies with real camera files. Windows/macOS packages, signed installers and official Launcher registration have not been produced. New player copy is English. Upstream binary updates are disabled to avoid replacing this fork.

Original MIT / Apache-2.0 notices are retained. Restricted ArtCraft wordmarks are removed. See [validation notes](docs/YOUCRAFT.md) and the [upstream architecture](docs/architecture.md).

Player shortcuts: **Space/K** play/pause, **J/L** −/+10 seconds, **←/→** −/+5 seconds,
**↑/↓** volume, **M** mute, **F** fullscreen, **T** theater, **C** captions, **I** miniplayer.
Back returns to the library with a floating miniplayer. Click its expand icon to resume the full view,
or its cross to stop playback. Player controls hide after 2.5 seconds without input while playing.
Hover the progress bar for a timestamp and a thumbnail preview (loaded after a short hover delay).
Favorites from older libraries become **Liked videos**.

Theater mode includes a scrollable video grid below the movie. F/T/I preserve independent viewing
preferences. Video boxes follow the source aspect ratio; thumbnails fill their tiles using centered
cropping instead of stretching. Preview frames are prepared in the background and cached on disk
for later sessions; first-time preparation still takes time.
