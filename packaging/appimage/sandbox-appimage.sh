#!/usr/bin/env bash
# Repack a Tauri-built AppImage so it starts through our sandboxing AppRun.
#
#   packaging/appimage/sandbox-appimage.sh <input.AppImage> <output.AppImage>
#
# Needs appimagetool on PATH (or APPIMAGETOOL pointing to it).
set -euo pipefail

in="$(readlink -f "$1")"
out="$(readlink -f "$(dirname "$2")")/$(basename "$2")"
here="$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")"
tool="${APPIMAGETOOL:-appimagetool}"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
cd "$work"
"$in" --appimage-extract >/dev/null
root="$work/squashfs-root"

# Keep linuxdeploy's launcher (it sets up GTK/WebKit paths) and start it from ours.
mv "$root/AppRun" "$root/AppRun.unsandboxed"
install -m 0755 "$here/AppRun" "$root/AppRun"

# Ship our desktop entry instead of the minimal one Tauri generates (proper
# name, mailto: handler); desktop integration tools read it from the AppImage.
desktop="$(ls "$root"/*.desktop | head -1)"
install -m 0644 "$here/../linux/moho-mail.desktop" "$desktop"

# appimagetool insists that everything is readable by all users; some files
# come out of the build as 0700. `X` only adds execute to directories and to
# files that are already executable.
chmod 0755 "$root"
chmod -R a+rX "$root"
chmod 0755 "$root/AppRun" "$root/AppRun.unsandboxed"

ARCH="${ARCH:-$(uname -m)}" "$tool" --no-appstream "$root" "$out"
echo "wrote $out"
