#!/usr/bin/env bash
# Fails on any `uses:` of this repository's workflows that is not pinned by a
# full commit SHA, with its version in a comment:
#
#   - uses: actions/checkout@<40-character SHA> # v7.0.1
#
# A tag or a branch can be moved to other code, a commit SHA cannot. A local
# action or workflow (`./...`) is part of this repository and needs no pin.
#
# Usage: check-workflow-pins.sh [file...]. Without files, every YAML file
# under .github/.
set -euo pipefail

if [[ $# -eq 0 ]]; then
  cd "$(dirname "$0")/.."
  mapfile -t files < <(git ls-files -- '.github/*.yml' '.github/*.yaml')
else
  files=("$@")
fi

uses='^[[:space:]]*(-[[:space:]]+)?uses:[[:space:]]*["'\'']?([^"'\''[:space:]#]+)'
pinned='@[0-9a-f]{40}$'
comment='#[[:space:]]*[^[:space:]]'

status=0
for file in "${files[@]}"; do
  n=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    n=$((n + 1))
    [[ "$line" =~ $uses ]] || continue
    ref="${BASH_REMATCH[2]}"
    [[ "$ref" == ./* ]] && continue
    if [[ ! "$ref" =~ $pinned ]]; then
      echo "$file:$n: $ref is not pinned by a full commit SHA; write it as <action>@<40-character SHA> # <version>"
      status=1
    elif [[ ! "${line#*"$ref"}" =~ $comment ]]; then
      echo "$file:$n: $ref has no version comment; add the version after the SHA: # <version>"
      status=1
    fi
  done < "$file"
done
exit "$status"
