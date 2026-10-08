#!/usr/bin/env bash
# Starts the GammaEngine test server prepared by setup.sh. Linux counterpart of start.ps1.
#
# Usage: tools/test-server/start.sh [--dir DIR] [--java PATH] [--memory 4G] [--no-console] [-- extra JVM arguments]
#   Example on Java 21: tools/test-server/start.sh --java /opt/jdk-21/bin/java -- -XX:+UseZGC -XX:+ZGenerational
#   --no-console starts without reading the console, for a server run in the background.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
dir="$repo/test-server"
java="java"
memory="4G"
server_args=()
while [ $# -gt 0 ]; do
    case "$1" in
        --dir) dir="$2"; shift 2 ;;
        --java) java="$2"; shift 2 ;;
        --memory) memory="$2"; shift 2 ;;
        --no-console) server_args+=(--noconsole); shift ;;
        --) shift; break ;;
        *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
done
[ -f "$dir/server.jar" ] || { echo "No server.jar in $dir. Run tools/test-server/setup.sh first." >&2; exit 1; }

version_line="$("$java" -version 2>&1 | head -n 1)"
major="$(echo "$version_line" | sed -E 's/.*version "([0-9]+)(\.([0-9]+))?.*/\1 \3/')"
read -r first second <<< "$major"
if [ "$first" = 1 ]; then first="$second"; fi

arguments=("-Xms$memory" "-Xmx$memory")
if [ "$first" -ge 9 ]; then
    # The module system closes what Forge and the mods reach into: java9args.txt opens it again.
    arguments+=("@$repo/java9args.txt")
fi
# nogui last: FML's argument parser drops an option without a value when it comes last.
arguments+=("$@" -jar server.jar "${server_args[@]}" nogui)

echo "[GammaEngine] Java $first ($version_line)"
echo "[GammaEngine] $java ${arguments[*]}"
cd "$dir"
exec "$java" "${arguments[@]}"
