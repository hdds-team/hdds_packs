#!/usr/bin/env bash
# Refresh docs/screenshots/ by running each demo and photographing its viewer.
#
# Every image in the README comes from here: the simulator publishes on DDS,
# the bridge relays to the browser, and the shot is taken once the page has
# actually received samples. Nothing is staged and no image is hand-made.
#
#   make build                         # the simulators
#   (cd <demo>/<viewer> && npm install && npm run build)
#   hdds-ws on PATH, plus google-chrome (or chromium), node and python3
#   tools/capture-screenshots.sh [demo ...]
#
# With no argument every web viewer is captured; otherwise only those named.
set -u

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
OUT=$ROOT/docs/screenshots
TMP=${TMPDIR:-/tmp}/hdds-demos-capture.$$
DOMAIN=${DOMAIN:-99}
BRIDGE_PORT=9090      # the viewers hardcode this one
HTTP_PORT=${HTTP_PORT:-4173}
DEVTOOLS_PORT=${DEVTOOLS_PORT:-9222}
SETTLE=${SETTLE:-8}

# Keep DDS on the loopback and make sure nothing can hand a URL to a desktop
# browser: the npm `open` package consults $BROWSER before xdg-open.
export HDDS_INTERFACE=${HDDS_INTERFACE:-lo}
export BROWSER=/bin/true

CHROME=$(command -v google-chrome || command -v chromium || command -v chromium-browser || true)
for tool in hdds-ws node python3; do
    command -v "$tool" >/dev/null || { echo "missing: $tool"; exit 1; }
done
[ -n "$CHROME" ] || { echo "missing: google-chrome or chromium"; exit 1; }

mkdir -p "$OUT" "$TMP"
trap 'rm -rf "$TMP"' EXIT
cd "$ROOT" || exit 1

free_ports() {
    for port in "$BRIDGE_PORT" "$HTTP_PORT" "$DEVTOOLS_PORT"; do
        for pid in $(ss -ltnp 2>/dev/null | grep -E ":$port " \
                     | grep -oE 'pid=[0-9]+' | cut -d= -f2 | sort -u); do
            kill "$pid" 2>/dev/null
            sleep 1
            kill -0 "$pid" 2>/dev/null && kill -9 "$pid" 2>/dev/null
        done
    done
}

# Every publisher of a demo that is not a desktop application.
producers_of() {
    local demo=$1 member name target
    for member in "$demo"/*/; do
        [ -f "$member/src/main.rs" ] || continue
        grep -q '\.writer()' "$member/src/main.rs" || continue
        grep -qE '^(gtk4|eframe|egui|fltk|ratatui|crossterm)' "$member/Cargo.toml" && continue
        name=$(sed -n 's/^name = "\(.*\)"/\1/p' "$member/Cargo.toml" | head -1)
        target=${CARGO_TARGET_DIR:-$demo/target}/release/$name
        [ -x "$target" ] && echo "$target"
    done
}

only=$*
full=0; thin=0; skipped=0
for manifest in $(git ls-files '*/package.json' | grep -v node_modules); do
    viewer=$(dirname "$manifest")
    demo=${viewer%%/*}
    [ -z "$only" ] || case " $only " in *" $demo "*) ;; *) continue ;; esac

    if [ ! -d "$viewer/dist" ]; then
        printf '%-28s no built viewer, run npm run build\n' "$demo"
        skipped=$((skipped + 1)); continue
    fi
    mapfile -t producers < <(producers_of "$demo")
    if [ ${#producers[@]} -eq 0 ]; then
        printf '%-28s no built simulator, run make build\n' "$demo"
        skipped=$((skipped + 1)); continue
    fi

    free_ports
    hdds-ws --domain "$DOMAIN" --port "$BRIDGE_PORT" --bind 127.0.0.1 \
        > "$TMP/bridge.log" 2>&1 &
    bridge=$!
    sleep 2
    sims=()
    for producer in "${producers[@]}"; do
        "$producer" > "$TMP/$(basename "$producer").log" 2>&1 &
        sims+=($!)
    done
    sleep 4

    # A plain static server, never the dev server: several viewers carry
    # server.open:true in their vite config, which preview inherits, and it
    # opens a tab in the desktop browser on every capture.
    (cd "$viewer/dist" && python3 -m http.server "$HTTP_PORT" --bind 127.0.0.1 \
        > "$TMP/http.log" 2>&1) &
    http=$!
    for _ in $(seq 1 20); do
        curl -fsS -o /dev/null "http://127.0.0.1:$HTTP_PORT/" 2>/dev/null && break
        sleep 1
    done

    "$CHROME" --headless=new --no-sandbox --disable-gpu --disable-dev-shm-usage \
        --use-gl=swiftshader --enable-unsafe-swiftshader --hide-scrollbars \
        --remote-debugging-port="$DEVTOOLS_PORT" --remote-allow-origins='*' \
        about:blank > "$TMP/chrome.log" 2>&1 &
    chrome=$!
    for _ in $(seq 1 20); do
        curl -fsS -o /dev/null "http://127.0.0.1:$DEVTOOLS_PORT/json" 2>/dev/null && break
        sleep 1
    done

    printf '%-28s ' "$demo"
    if node "$ROOT/tools/capture-shot.mjs" "http://127.0.0.1:$HTTP_PORT/" \
            "$OUT/$demo.png" --min 40 --settle "$SETTLE"; then
        full=$((full + 1))
    else
        thin=$((thin + 1))
    fi

    kill "$chrome" "$http" "${sims[@]}" "$bridge" 2>/dev/null
    sleep 1
    for pid in "$chrome" "$http" "${sims[@]}" "$bridge"; do
        kill -0 "$pid" 2>/dev/null && kill -9 "$pid" 2>/dev/null
    done
done
free_ports

echo
echo "=== $full captured, $thin thin, $skipped skipped ==="
[ "$thin" -eq 0 ] || exit 1
