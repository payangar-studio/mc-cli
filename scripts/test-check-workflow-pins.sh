#!/usr/bin/env bash
# Runs check-workflow-pins.sh on deliberate defects and on the closest
# legitimate cases.
set -euo pipefail
check="$(cd "$(dirname "$0")" && pwd)/check-workflow-pins.sh"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
sha=3d3c42e5aac5ba805825da76410c181273ba90b1

failures=0
expect() {
  local want=$1 name=$2 line=$3 got=0
  printf 'jobs:\n  job:\n    steps:\n%s\n' "$line" > "$tmp/$name.yml"
  bash "$check" "$tmp/$name.yml" > "$tmp/$name.out" || got=$?
  if [[ "$want" == pass && $got -ne 0 ]] || [[ "$want" == fail && $got -eq 0 ]]; then
    echo "FAIL $name: expected the check to $want on: $line"
    cat "$tmp/$name.out"
    failures=$((failures + 1))
  fi
}

expect pass pinned "      - uses: actions/checkout@$sha # v7.0.1"
expect pass pinned-quoted "      - uses: \"actions/checkout@$sha\" # v7.0.1"
expect pass pinned-key "        uses: actions/upload-artifact@$sha # v7.0.2"
expect pass local-action "      - uses: ./.github/actions/setup"
expect pass reusable-pinned "    uses: payangar-studio/mc-cli/.github/workflows/publish-modpack.yml@$sha # v0.4.0"
expect fail tag "      - uses: actions/checkout@v7"
expect fail branch "      - uses: dtolnay/rust-toolchain@stable"
expect fail short-sha "      - uses: actions/checkout@${sha:0:7} # v7.0.1"
expect fail uppercase-sha "      - uses: actions/checkout@${sha^^} # v7.0.1"
expect fail no-ref "      - uses: actions/checkout"
expect fail no-comment "      - uses: actions/checkout@$sha"
expect fail reusable-tag "    uses: payangar-studio/mc-cli/.github/workflows/publish-modpack.yml@v0.4.0"
expect fail quoted-tag "        uses: 'actions/upload-artifact@v7'"
expect fail flow-tag "      - {uses: actions/checkout@v7}"
expect fail flow-second-key-tag "      - {name: Checkout, uses: actions/checkout@v7}"
expect fail quoted-key-tag "      - \"uses\": actions/checkout@v7"
expect fail comment-not-version "      - uses: actions/checkout@$sha # TODO"
expect fail comment-branch "      - uses: dtolnay/rust-toolchain@$sha # stable"
expect pass flow-pinned "      - {uses: actions/checkout@$sha} # v7.0.1"
expect pass quoted-key-pinned "      - 'uses': actions/checkout@$sha # v7.0.1"

if [[ $failures -ne 0 ]]; then
  echo "$failures case(s) failed."
  exit 1
fi
echo "check-workflow-pins: every case behaves."
