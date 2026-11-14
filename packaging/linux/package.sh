#!/usr/bin/env bash
# Build and package YouCraft for Linux (<arch> is x86_64 or aarch64, or riscv64 when cross-compiling):
#
#   $DIST/youcraft-<version>-linux-<arch>.AppImage  any distro with glibc >= the build host's
#   $DIST/youcraft-<version>-linux-<arch>.AppImage.zsync  delta updates (needs zsyncmake)
#   $DIST/youcraft-<version>-linux-<arch>.deb       Debian, Ubuntu, Mint, Pop!_OS, ...
#   $DIST/youcraft-<version>-linux-<arch>.rpm       Fedora, openSUSE, RHEL, ...
#   $DIST/youcraft-<version>-linux-<arch>.tar.gz    plain FHS-style tree (bin/, share/)
#
# Usage: packaging/linux/package.sh [--skip-build] [--formats "appimage deb rpm tar"]
#
# Needs: cargo; nfpm for deb/rpm (https://nfpm.goreleaser.com); appimagetool for the AppImage
# (downloaded into $CARGO_TARGET_DIR if missing). Build on an old distro (CI: Ubuntu 22.04,
# glibc 2.35) so the binaries run on newer ones. Optional: desktop-file-validate, appstreamcli,
# zsyncmake (the zsync package) for the AppImage's .zsync.
#
# Cross-compiling (CI's riscv64 job, tar only): set CROSS_ARCH (riscv64), CROSS_TARGET (the Rust
# target), CROSS_COMPILE (binutils prefix, for strip) and optionally EMULATOR (qemu-riscv64) to
# smoke-test the CLI.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"
APP_ID=local.youcraft.player

SKIP_BUILD=0
FORMATS="appimage deb rpm tar"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --formats) [[ $# -ge 2 && -n "$2" ]] || { echo "error: --formats needs a format list" >&2; exit 2; }; FORMATS="$2"; shift 2 ;;
    -h | --help) sed -n '2,21p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

[[ "$FORMATS" =~ [^[:space:]] ]] || { echo "error: --formats needs a format list" >&2; exit 2; }
for format in $FORMATS; do
  case "$format" in appimage|deb|rpm|tar) ;; *) echo "error: unknown format $format" >&2; exit 2 ;; esac
done
has() { local fmt="$1"; case " $FORMATS " in *" $fmt "*) return 0 ;; *) return 1 ;; esac; }
if has deb || has rpm; then
  command -v nfpm >/dev/null || { echo "error: nfpm is required; see packaging/README.md" >&2; exit 1; }
fi

ARCH="${CROSS_ARCH:-$(uname -m)}"
case "$ARCH" in
  riscv64) DEB_ARCH=riscv64 ;;
  x86_64) DEB_ARCH=amd64 ;;
  aarch64 | arm64) ARCH=aarch64; DEB_ARCH=arm64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
export YOUCRAFT_MAINTAINER="${YOUCRAFT_MAINTAINER:-YouCraft contributors}"
BASENAME="youcraft-$VERSION-linux-$ARCH"

echo "==> YouCraft $VERSION for Linux $ARCH ($FORMATS)"

if [[ "$SKIP_BUILD" = 0 ]]; then
  if [[ -n "${CROSS_TARGET:-}" ]]; then
    (cd "$ROOT" && cargo build --release --locked -p filmcraft -p filmcraft-cli --target "$CROSS_TARGET")
  else
    (cd "$ROOT" && cargo build --release --locked -p filmcraft -p filmcraft-cli)
  fi
fi
BIN="$CARGO_TARGET_DIR/${CROSS_TARGET:+$CROSS_TARGET/}release"
for binary in youcraft filmcraft-cli; do
  [[ -x "$BIN/$binary" ]] || { echo "error: missing $BIN/$binary; run without --skip-build first" >&2; exit 1; }
done
# A build on a newer distro must not advertise the old upstream glibc floor.
command -v readelf >/dev/null || { echo "error: readelf (binutils) is required" >&2; exit 1; }
YOUCRAFT_GLIBC_MIN="$(readelf --version-info "$BIN/youcraft" "$BIN/filmcraft-cli" |
  sed -n 's/.*Name: GLIBC_\([0-9][0-9.]*\).*/\1/p' | sort -Vu | tail -n 1)"
[[ -n "$YOUCRAFT_GLIBC_MIN" ]] || { echo "error: could not determine the glibc requirement" >&2; exit 1; }
export YOUCRAFT_GLIBC_MIN
WORK="$CARGO_TARGET_DIR/linux-package"
STAGE="$WORK/root"
rm -rf "$WORK"

# ---- stage an FHS tree (shared by every format) -------------------------------------------------
install -Dm755 "$BIN/youcraft" "$STAGE/usr/bin/youcraft"
install -Dm755 "$BIN/filmcraft-cli" "$STAGE/usr/bin/youcraft-cli"
"${CROSS_COMPILE:-}strip" "$STAGE/usr/bin/youcraft" "$STAGE/usr/bin/youcraft-cli" 2>/dev/null || true
install -Dm644 "$HERE/$APP_ID.desktop" "$STAGE/usr/share/applications/$APP_ID.desktop"
install -Dm644 "$HERE/$APP_ID.mime.xml" "$STAGE/usr/share/mime/packages/$APP_ID.xml"
mkdir -p "$STAGE/usr/share/metainfo"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$FILMCRAFT_BUILD_DATE/g" \
  "$HERE/$APP_ID.metainfo.xml.in" >"$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
mkdir -p "$STAGE/usr/share/icons"
# Reuse the attributed, open-source app icon under this fork's desktop identity.
while IFS= read -r icon; do
  relative="${icon#"$ROOT/assets/app-icon/hicolor/"}"
  relative="${relative/ai.storyteller.filmcraft/$APP_ID}"
  install -Dm644 "$icon" "$STAGE/usr/share/icons/hicolor/$relative"
done < <(find "$ROOT/assets/app-icon/hicolor" -type f \( -name '*.png' -o -name '*.svg' \))
mkdir -p "$STAGE/usr/share/doc/youcraft"
copy_docs "$STAGE/usr/share/doc/youcraft"

if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$STAGE/usr/share/applications/$APP_ID.desktop"
fi
if command -v appstreamcli >/dev/null; then
  # This local fork has no public homepage; do not advertise upstream as its support site.
  appstreamcli validate --no-net --override url-homepage-missing=info --explain "$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
fi

# ---- .tar.gz ------------------------------------------------------------------------------------
if has tar; then
  mkdir -p "$WORK/tar"
  cp -R "$STAGE/usr" "$WORK/tar/$BASENAME"
  install -m755 "$HERE/install.sh" "$WORK/tar/$BASENAME/install.sh"
  tar -C "$WORK/tar" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
  echo "wrote $DIST/$BASENAME.tar.gz"
fi

# ---- .deb / .rpm --------------------------------------------------------------------------------
if has deb || has rpm; then
  command -v nfpm >/dev/null || { echo "error: nfpm not found (https://nfpm.goreleaser.com/install/)" >&2; exit 1; }
  export VERSION
  export NFPM_ARCH="$DEB_ARCH"
  # nfpm expands env vars in fields like `version` and `arch`, but not in `contents[].src`.
  sed "s|\${STAGE}|$STAGE|g" "$HERE/nfpm.yaml" >"$WORK/nfpm.yaml"
  for fmt in deb rpm; do
    if has "$fmt"; then (cd "$ROOT" && nfpm package -f "$WORK/nfpm.yaml" -p "$fmt" -t "$DIST/$BASENAME.$fmt"); fi
  done
fi

# ---- AppImage -----------------------------------------------------------------------------------
if has appimage; then
  APPIMAGETOOL_VERSION=1.9.1
  # appimagetool otherwise embeds type2-runtime's unchecked `continuous` build in every AppImage.
  RUNTIME_VERSION=20251108
  case "$ARCH" in
    x86_64)
      APPIMAGETOOL_SHA256=ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0
      RUNTIME_SHA256=2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d
      ;;
    aarch64)
      APPIMAGETOOL_SHA256=f0837e7448a0c1e4e650a93bb3e85802546e60654ef287576f46c71c126a9158
      RUNTIME_SHA256=00cbdfcf917cc6c0ff6d3347d59e0ca1f7f45a6df1a428a0d6d8a78664d87444
      ;;
    *) APPIMAGETOOL_SHA256='' RUNTIME_SHA256='' ;;
  esac
  APPDIR="$WORK/YouCraft.AppDir"
  cp -R "$STAGE" "$APPDIR"
  ln -s usr/bin/youcraft "$APPDIR/AppRun"
  cp "$HERE/$APP_ID.desktop" "$APPDIR/$APP_ID.desktop"
  cp "$STAGE/usr/share/icons/hicolor/256x256/apps/$APP_ID.png" "$APPDIR/$APP_ID.png"
  ln -s "$APP_ID.png" "$APPDIR/.DirIcon"

  if [[ -z "$RUNTIME_SHA256" ]]; then
    echo "no pinned AppImage runtime for $ARCH" >&2
    exit 2
  fi
  RUNTIME="$CARGO_TARGET_DIR/appimage-runtime-$RUNTIME_VERSION-$ARCH"
  if [[ ! -f "$RUNTIME" ]]; then
    download="$RUNTIME.download"
    curl --proto '=https' -fsSL -o "$download" \
      "https://github.com/AppImage/type2-runtime/releases/download/$RUNTIME_VERSION/runtime-$ARCH"
    printf '%s  %s\n' "$RUNTIME_SHA256" "$download" | sha256sum -c -
    mv "$download" "$RUNTIME"
  fi
  printf '%s  %s\n' "$RUNTIME_SHA256" "$RUNTIME" | sha256sum -c -

  TOOL="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
  if [[ -z "$TOOL" ]]; then
    if [[ -z "$APPIMAGETOOL_SHA256" ]]; then
      echo "no pinned appimagetool for $ARCH; set APPIMAGETOOL or put appimagetool on PATH" >&2
      exit 2
    fi
    TOOL="$CARGO_TARGET_DIR/appimagetool-$APPIMAGETOOL_VERSION-$ARCH.AppImage"
    if [[ ! -x "$TOOL" ]]; then
      download="$TOOL.download"
      curl --proto '=https' -fsSL -o "$download" \
        "https://github.com/AppImage/appimagetool/releases/download/$APPIMAGETOOL_VERSION/appimagetool-$ARCH.AppImage"
      printf '%s  %s\n' "$APPIMAGETOOL_SHA256" "$download" | sha256sum -c -
      chmod 755 "$download"
      mv "$download" "$TOOL"
    fi
    printf '%s  %s\n' "$APPIMAGETOOL_SHA256" "$TOOL" | sha256sum -c -
  fi
  # Absolute, because appimagetool runs in $DIST below (CARGO_TARGET_DIR or APPIMAGETOOL may be
  # relative, e.g. target/agent-<name>).
  OUT="$(cd "$DIST" && pwd)/$BASENAME.AppImage"
  APPDIR="$(cd "$APPDIR" && pwd)"
  TOOL="$(cd "$(dirname "$TOOL")" && pwd)/$(basename "$TOOL")"
  RUNTIME="$(cd "$(dirname "$RUNTIME")" && pwd)/$(basename "$RUNTIME")"
  # A .zsync left by an earlier run would hide a missing zsyncmake and describe another file.
  rm -f "$OUT.zsync"
  # Update information: AppImageUpdate, AppImageLauncher and the like read it from the file and
  # fetch only the blocks that changed in a newer release, through the .zsync published next to
  # each AppImage on GitHub Releases. `latest` is the newest published release that is not a
  # pre-release. A fork's builds opt in with YOUCRAFT_UPDATE_REPOSITORY.
  # Local fork builds must never fetch upstream FilmCraft releases.
  update_args=()
  if [[ -n "${YOUCRAFT_UPDATE_REPOSITORY:-}" ]]; then
    REPO="$YOUCRAFT_UPDATE_REPOSITORY"
    [[ "$REPO" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ && "$REPO" != storytold/filmcraft ]] || {
      echo "error: YOUCRAFT_UPDATE_REPOSITORY must name this fork's owner/repository" >&2; exit 2;
    }
    update_args=(-u "gh-releases-zsync|${REPO%%/*}|${REPO#*/}|latest|youcraft-*-linux-$ARCH.AppImage.zsync")
  fi
  # Extract-and-run: works without FUSE (containers, CI). The output embeds the static runtime,
  # so users don't need libfuse2 either. With zsyncmake on the host (CI installs the zsync
  # package) appimagetool also writes the .zsync, into its working directory, hence the cd.
  (cd "$DIST" && ARCH="$ARCH" APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream --runtime-file "$RUNTIME" "${update_args[@]}" "$APPDIR" "$OUT")
  echo "wrote $OUT"
  if [[ -s "$OUT.zsync" ]]; then
    echo "wrote $OUT.zsync"
  elif [[ ${#update_args[@]} -gt 0 ]]; then
    warn "zsyncmake not found, so $OUT.zsync was not written; AppImage delta updates need it"
  fi
fi

if [[ -z "${CROSS_TARGET:-}" ]]; then
  "$STAGE/usr/bin/youcraft" --version
  "$STAGE/usr/bin/youcraft-cli" --version
elif [[ -n "${EMULATOR:-}" ]]; then
  "$EMULATOR" "$STAGE/usr/bin/youcraft-cli" --version
fi
echo "==> done"
ls -lh "$DIST"
