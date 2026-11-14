# YouCraft packaging

YouCraft follows the existing Crafting Apps Linux pipeline: one staged filesystem tree, nFPM
for DEB/RPM, a pinned AppImage tool/runtime, and a portable tarball. The Flatpak bundle consumes
that tarball. This matches the structure used by
[PhotoCraft](https://github.com/storytold/photocraft/blob/main/packaging/linux/package.sh) and
[VectorCraft](https://github.com/storytold/vectorcraft/blob/main/packaging/linux/package.sh).
The product and decoder code remain Rust; these are build-time packaging tools.

## Build on Linux

Run from the repository root. Install Rust using the checked-in toolchain, the native development
dependencies in [setup](../docs/contributing.md), NASM for AV1 on x86, and `binutils` (`strip` and
`readelf`). DEB/RPM additionally require [nFPM](https://github.com/goreleaser/nfpm/releases/tag/v2.49.0)
on `PATH` (validated with 2.49.0). The local validation tool is stored in the ignored
`target/packaging-tools/`; in this checkout, enable it with `export PATH="$PWD/target/packaging-tools:$PATH"`.
For a fresh checkout, use the official release matching your host architecture and
verify it against the release's `checksums.txt`. AppImage tools are downloaded with pinned hashes;
`curl`, `tar`, and `sha256sum` must be available. `zsyncmake` is optional.

```sh
# Build both native binaries and produce all four Linux formats:
bash packaging/linux/package.sh

# Or choose the formats you need:
bash packaging/linux/package.sh --formats "deb rpm tar"
bash packaging/linux/package.sh --formats appimage

# Repackage existing release binaries without rebuilding:
bash packaging/linux/package.sh --skip-build --formats deb
```

The default build runs `cargo build --release --locked -p filmcraft -p filmcraft-cli`.
Cargo package names remain compatible with the shared engine; installed executables are
`youcraft` and `youcraft-cli`. Missing tools and unknown format names fail before the build.

Outputs are in `dist/release/`, named `youcraft-<version>-linux-<arch>` with `.deb`, `.rpm`,
`.AppImage`, or `.tar.gz`. Version comes from the workspace Cargo.toml. `DIST` and
`CARGO_TARGET_DIR` can override output/build directories; `YOUCRAFT_VERSION` overrides package
version only. `YOUCRAFT_MAINTAINER` can set the package maintainer; its default is
`YouCraft contributors`, without claiming upstream maintainership. The desktop application ID is `local.youcraft.player`, matching the native window.
The shared `.fcproj` file type remains compatible. Bundled licenses and attribution are installed
under `share/doc/youcraft/`, including rav1d and font notices.

DEB/RPM metadata uses the glibc requirement read from the built executables. For broad Linux
compatibility, build on the oldest distribution you intend to support; compiling on a newer host
does not create an Ubuntu 22.04-compatible binary automatically. Cross-builds retain the inherited
`CROSS_TARGET`, `CROSS_ARCH`, and `CROSS_COMPILE` settings and require matching target libraries.

## Install or run

For version 0.14.0 on x86_64, from the repository root:

```sh
sudo apt install ./dist/release/youcraft-0.14.0-linux-x86_64.deb
# Fedora and related RPM distributions:
sudo dnf install ./dist/release/youcraft-0.14.0-linux-x86_64.rpm
# Or run the portable AppImage:
chmod +x dist/release/youcraft-0.14.0-linux-x86_64.AppImage
./dist/release/youcraft-0.14.0-linux-x86_64.AppImage
```

For a tarball, extract it and run its `install.sh`; the default destination is `~/.local`, with
`--prefix /absolute/path` available. Launch from the application menu or run `youcraft`.
Local packages are unsigned development builds. They do not register the fork with ArtCraft's Launcher.
AppImage update metadata is omitted unless `YOUCRAFT_UPDATE_REPOSITORY=owner/repository` explicitly
names this fork's release repository; upstream FilmCraft is never used as the default.

## Flatpak and other platforms

After creating the Linux tarball, install `flatpak`, `flatpak-builder`, and `librsvg2-common`, then:

```sh
bash packaging/linux/flatpak-bundle.sh --no-test
```

This downloads the Freedesktop runtime/SDK as needed and produces a `.flatpak` in `dist/release`.
Both executables are checked inside the selected runtime before the bundle is written. A binary
built against a newer glibc than that runtime must be rebuilt on a compatible host.
Without `--no-test`, the inherited script also installs the bundle for the current user and runs
its smoke checks. The bundle is a local distribution route, not an official Flathub listing.
The inherited from-source Flathub manifest still describes upstream FilmCraft.

Windows MSI/ZIP, macOS DMG, FreeBSD, and web packaging remain inherited upstream scripts and are
not yet adapted or validated as YouCraft release paths. They require their respective build hosts.
The current packaging work covers the shared Linux formats; it does not change release CI.
