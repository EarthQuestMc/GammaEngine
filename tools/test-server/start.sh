#!/usr/bin/env bash
# Starts the GammaEngine test server prepared by setup.sh. Linux counterpart of start.ps1.
#
# Usage: tools/test-server/start.sh [--dir DIR] [--java PATH] [--memory 4G] [--gc zgc|g1] [--gc-log]
#                                    [--no-console] [-- extra JVM arguments]
#   Example on Java 21: tools/test-server/start.sh --java /opt/jdk-21/bin/java --gc zgc
#   --gc zgc needs Java 15 or later and adds -XX:+ZGenerational on Java 21 and 22 (the default from 23).
#   --gc-log writes the GC log, with safepoint pauses, to logs/gc-<pid>.log: JMX rounds pauses to whole ms.
#   --no-console starts without reading the console, for a server run in the background.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
dir="$repo/test-server"
java="java"
memory="4G"
gc=""
gc_log=0
server_args=()
while [ $# -gt 0 ]; do
    case "$1" in
        --dir) dir="$2"; shift 2 ;;
        --java) java="$2"; shift 2 ;;
        --memory) memory="$2"; shift 2 ;;
        --gc) gc="$2"; shift 2 ;;
        --gc-log) gc_log=1; shift ;;
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
case "$gc" in
    "") ;;
    zgc)
        [ "$first" -ge 15 ] || { echo "ZGC needs Java 15 or later; this is Java $first." >&2; exit 1; }
        arguments+=(-XX:+UseZGC)
        if [ "$first" -ge 21 ] && [ "$first" -le 22 ]; then arguments+=(-XX:+ZGenerational); fi ;;
    g1) arguments+=(-XX:+UseG1GC) ;;
    *) echo "Unknown collector: $gc (zgc or g1)" >&2; exit 2 ;;
esac
if [ "$gc_log" = 1 ]; then
    mkdir -p "$dir/logs"
    if [ "$first" -ge 9 ]; then
        arguments+=("-Xlog:gc,safepoint:file=logs/gc-%p.log:uptime,level,tags")
    else
        arguments+=("-Xloggc:logs/gc-%p.log" -XX:+PrintGCDetails -XX:+PrintGCApplicationStoppedTime)
    fi
fi
# nogui last: FML's argument parser drops an option without a value when it comes last.
arguments+=("$@" -jar server.jar "${server_args[@]}" nogui)

echo "[GammaEngine] Java $first ($version_line)"
echo "[GammaEngine] $java ${arguments[*]}"
cd "$dir"
exec "$java" "${arguments[@]}"
