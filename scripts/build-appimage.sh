#!/usr/bin/env bash
# Build the sandboxed AppImage the way CI does, then install it for this user
# with a desktop entry and icons so it starts from the application launcher.
#
#   scripts/build-appimage.sh               build and install
#   scripts/build-appimage.sh --no-install  only build (dist/Moho_Mail-<version>-x86_64.AppImage)
#
# Installs to ~/.local/bin/MohoMail.AppImage and
# ~/.local/share/applications/moho-mail.desktop.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
install_it=1
[[ "${1:-}" == --no-install ]] && install_it=0

bridge_version="${PROTON_BRIDGE_VERSION:-$(sed -n 's/^ *PROTON_BRIDGE_VERSION: "\(.*\)"/\1/p' .github/workflows/appimage.yml)}"
export PROTON_BRIDGE_SRC="${PROTON_BRIDGE_SRC:-$root/third_party/proton-bridge}"
if [[ ! -f "$PROTON_BRIDGE_SRC/go.mod" ]]; then
  git clone --quiet --depth 1 --branch "v$bridge_version" https://github.com/ProtonMail/proton-bridge "$PROTON_BRIDGE_SRC"
fi
# linuxdeploy's bundled strip cannot handle binaries from newer toolchains.
export NO_STRIP=true

if command -v pnpm >/dev/null; then pnpm=(pnpm); else pnpm=(npx -y pnpm@10); fi
"${pnpm[@]}" install --frozen-lockfile
"${pnpm[@]}" tauri build --bundles appimage --features proton-bridge

# appimagetool: from PATH, $APPIMAGETOOL, or the copy inside the
# linuxdeploy plugin that `tauri build` just used.
tool="${APPIMAGETOOL:-$(command -v appimagetool || true)}"
if [[ -z "$tool" ]]; then
  plugin="${XDG_CACHE_HOME:-$HOME/.cache}/tauri/linuxdeploy-plugin-appimage.AppImage"
  cache="$root/target/appimagetool"
  if [[ ! -x "$cache/squashfs-root/usr/bin/appimagetool" ]]; then
    rm -rf "$cache" && mkdir -p "$cache"
    (cd "$cache" && "$plugin" --appimage-extract >/dev/null)
  fi
  tool="$cache/squashfs-root/usr/bin/appimagetool"
fi

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
out="$root/dist/Moho_Mail-$version-x86_64.AppImage"
mkdir -p dist
APPIMAGETOOL="$tool" ARCH=x86_64 packaging/appimage/sandbox-appimage.sh \
  target/release/bundle/appimage/moho-mail_"$version"_amd64.AppImage "$out"
(cd dist && sha256sum "$(basename "$out")" > SHA256SUMS)

[[ $install_it == 1 ]] || exit 0

bin_dir="${XDG_BIN_HOME:-$HOME/.local/bin}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
app="$bin_dir/MohoMail.AppImage"
install -Dm755 "$out" "$app"

appdir="target/release/bundle/appimage/moho-mail.AppDir"
for png in "$appdir"/usr/share/icons/hicolor/*/apps/moho-mail.png; do
  size="$(basename "$(dirname "$(dirname "$png")")")"
  install -Dm644 "$png" "$data/icons/hicolor/$size/apps/moho-mail.png"
done
install -Dm644 packaging/linux/moho-mail.svg "$data/icons/hicolor/scalable/apps/moho-mail.svg"

mkdir -p "$data/applications"
sed -e "s|^Exec=moho-mail|Exec=\"$app\"|" -e "/^Exec=/a TryExec=$app" \
  packaging/linux/moho-mail.desktop > "$data/applications/moho-mail.desktop"
command -v desktop-file-validate >/dev/null && desktop-file-validate "$data/applications/moho-mail.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database -q "$data/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -qtf "$data/icons/hicolor" || true

echo "installed $app"
echo "desktop entry $data/applications/moho-mail.desktop"
