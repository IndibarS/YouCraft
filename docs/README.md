# YouCraft documentation

YouCraft is a native local-video player built on the shared FilmCraft engine. The inherited
editor documentation describes shared internals and upstream features; it does not imply that
those features are exposed in the player.

| Start here | Purpose |
| --- | --- |
| [User guide and source layout](../README.md) | Launching, controls, module ownership, and current limitations |
| [Validation history](YOUCRAFT.md) | Dated checks and platform/codec limits; older entries are historical |
| [Contributor guide](../CONTRIBUTING.md) | Rules and development entry points |
| [Packaging](../packaging/README.md) | Linux package formats, prerequisites, and install commands |
| [Setup and build](contributing.md) | Toolchain, dependencies, and quality gates |
| [Architecture](architecture.md) | Shared workspace layers and engine design |
| [Testing](testing.md) | Player regressions and inherited test conventions |
| [Control protocol](control-protocol.md) | Authenticated automation API |
| [UI crate](../crates/ui-egui/README.md) | Player module boundaries and shared frontend behavior |
| [Media crate](../crates/media/README.md) | Media interfaces and embedded artwork |
| [Codec hub](../crates/codecs/README.md) | Decoder integration and failure handling |
| [Upstream provenance](UPSTREAM-README.md) | Source baseline, attribution, and compatibility names |

[ROADMAP.md](../ROADMAP.md), [roadmap detail](roadmap.md), and [gap analysis](gaps.md)
are inherited editor planning records. Their dates and parity estimates are historical, not a
YouCraft release commitment. Current player behavior and known limits are in the user guide.

Documentation should link relative to its own directory, distinguish current behavior from dated
validation, and preserve upstream licensing and attribution. Source formatting uses the root
`rustfmt.toml`; dependency boundaries are checked by `cargo xtask layers`.
