#!/usr/bin/env bash
# Prepares a GammaEngine test server from the latest build. Linux counterpart of setup.ps1.
#
# Usage: tools/test-server/setup.sh [--target DIR] [--accept-eula] [--reset-config] [--reset-world]
#   --accept-eula   writes eula=true; only pass it if you accept https://aka.ms/MinecraftEULA
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
target="$repo/test-server"
accept_eula=0
reset_config=0
reset_world=0
while [ $# -gt 0 ]; do
    case "$1" in
        --target) target="$2"; shift 2 ;;
        --accept-eula) accept_eula=1; shift ;;
        --reset-config) reset_config=1; shift ;;
        --reset-world) reset_world=1; shift ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
done

dist="$repo/build/distributions"
jar="$(ls -t "$dist"/*-server.jar 2>/dev/null | head -n 1 || true)"
[ -n "$jar" ] || { echo "No server jar in $dist. Run ./gradlew buildPackages first." >&2; exit 1; }
[ -f "$dist/libraries.zip" ] || { echo "libraries.zip is missing from $dist." >&2; exit 1; }

mkdir -p "$target"
cp -f "$jar" "$target/server.jar"
echo "[GammaEngine] Server jar: $(basename "$jar")"

# The server refuses a library without a matching .md5 next to it and then downloads it again;
# writing the checksums here keeps the first start offline.
mkdir -p "$target/libraries"
unzip -qo "$dist/libraries.zip" -d "$target/libraries"
count=0
while IFS= read -r -d '' library; do
    md5sum "$library" | cut -d ' ' -f 1 | tr -d '\n' > "$library.md5"
    count=$((count + 1))
done < <(find "$target/libraries" -name '*.jar' -print0)
echo "[GammaEngine] $count libraries in place"

for template in "$here"/config/*; do
    destination="$target/$(basename "$template")"
    if [ "$reset_config" = 1 ] || [ ! -e "$destination" ]; then
        cp -f "$template" "$destination"
        echo "[GammaEngine] Settings written: $(basename "$template")"
    fi
done

if [ "$accept_eula" = 1 ]; then
    echo "eula=true" > "$target/eula.txt"
elif ! grep -qs 'eula=true' "$target/eula.txt"; then
    echo "[GammaEngine] The Minecraft EULA is not accepted yet: run again with --accept-eula if you accept it."
fi

if [ "$reset_world" = 1 ]; then
    rm -rf "$target/world" "$target/world_nether" "$target/world_the_end"
    echo "[GammaEngine] Worlds removed"
fi

echo "[GammaEngine] Test server ready in $target"
echo "[GammaEngine] Start it with: tools/test-server/start.sh"
