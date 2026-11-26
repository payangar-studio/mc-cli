#!/bin/sh
# Checks that the pages of a design wiki are self-contained: each HTML file
# loads nothing besides itself, so that the wiki reads offline, from GitHub
# Pages or from a copy, with no CDN, font service or runtime behind it.
#
# Usage: check-page.sh <page.html>...
# Exit codes: 0 every page self-contained, 1 something is loaded, 2 usage.
#
# Allowed: `data:` URIs, `#` fragments, and links people follow (`<a href>`),
# to other pages too.
# Refused: any other `src`, `srcset` candidate, `poster`, `data`, `href` or
# `background` of an element that loads it, CSS `url()` and `@import`, module
# imports, workers, and network calls from scripts.

if [ "$#" -eq 0 ]; then
  echo "usage: check-page.sh <page.html>..." >&2
  exit 2
fi
for page in "$@"; do
  if [ ! -f "$page" ]; then
    echo "usage: check-page.sh <page.html>...: $page is not a file" >&2
    exit 2
  fi
done

# Checks one page; prints what it loads and returns 1 when it loads anything.
check() {
page=$1

# One line, so that a tag split over several lines is still one match.
text=$(tr '\r\n\t' '   ' <"$page")
q="[\"']"
quoted="(\"[^\"]*\"|'[^']*'"

# A value that loads nothing: a data URI or a fragment.
loads_nothing="^$q?[[:space:]]*(data:|#)"
# Elements that load what one of their attributes names.
tags='<(link|script|img|image|iframe|frame|embed|object|audio|video|source|track|input|use|feimage|base|body|table|thead|tbody|tfoot|tr|td|th)[[:space:]][^>]*>'
attrs="[[:space:]](src|srcset|poster|data|href|xlink:href|background)[[:space:]]*=[[:space:]]*$quoted|[^[:space:]>]+)"
network="fetch\(|import\(|importScripts\(|XMLHttpRequest|new (WebSocket|EventSource|Worker|SharedWorker)\(|serviceWorker\.register\(|sendBeacon\(|window\.claude|(import|export)[[:space:]][^;\"']*from[[:space:]]*$q|import[[:space:]]*$q"

values=$(printf '%s\n' "$text" | grep -oiE "$tags" | grep -oiE "$attrs")

problems=$(
  printf '%s\n' "$values" | grep -viE '^([[:space:]]*srcset|$)' |
    sed -E 's/^[[:space:]]*[^=]*=[[:space:]]*//' | grep -viE "$loads_nothing" |
    sed 's/^/loads a resource: /'
  # Each candidate of a srcset. A URL runs up to a space, so a data URI is cut
  # out whole first; any token left besides a descriptor (`2x`, `480w`) is a
  # URL that loads something.
  printf '%s\n' "$values" | grep -iE '^[[:space:]]*srcset' |
    sed -E 's/^[[:space:]]*[^=]*=[[:space:]]*//' | while IFS= read -r value; do
      rest=$(printf '%s\n' "$value" | sed -E 's/data:[^[:space:]]*//g' |
        tr "\"', " '\n\n\n\n' | grep -vE '^([0-9.]+[wxh])?$')
      [ -z "$rest" ] || echo "loads a resource: $value"
    done
  printf '%s\n' "$text" | grep -oiE "url\([[:space:]]*$quoted|[^)]*)" |
    sed -E 's/^url\([[:space:]]*//' | grep -viE "$loads_nothing" |
    sed 's/^/loads a resource from CSS: /'
  printf '%s\n' "$text" | grep -oiE '@import[^;]*' | sed 's/^/imports a stylesheet: /'
  printf '%s\n' "$text" | grep -oE "$network" | sed 's/^/calls the network: /'
)

if [ -n "$problems" ]; then
  printf '%s\n' "$problems" | while IFS= read -r line; do echo "$page: $line"; done
  echo "$page is not self-contained: inline what it loads (styles, scripts, SVG), or embed it as a data: URI" >&2
  return 1
fi
echo "$page is self-contained"
}

status=0
for page in "$@"; do
  check "$page" || status=1
done
exit $status
