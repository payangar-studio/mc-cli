#!/usr/bin/env bash
# Fails on any `uses:` of this repository's workflows that is not pinned by a
# full commit SHA, with its version in a comment:
#
#   - uses: actions/checkout@<40-character SHA> # v7.0.1
#
# A tag or a branch can be moved to other code, a commit SHA cannot. The
# comment is a version number (`v7.0.1`, `1.7.12`): a branch name or a note
# does not say which release the SHA is. A local action or workflow (`./...`)
# is part of this repository and needs no pin.
#
# The check reads lines, not YAML: it finds `uses` as a block key, quoted or
# not, and as a key of a flow mapping (`- {uses: ...}`).
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

uses='(^[[:space:]]*(-[[:space:]]+)?|[{,][[:space:]]*)["'\'']?uses["'\'']?[[:space:]]*:[[:space:]]*["'\'']?([^"'\''[:space:]#,{}]+)'
pinned='@[0-9a-f]{40}$'
version='#[[:space:]]*v?[0-9]+(\.[0-9]+)*([-+][0-9A-Za-z.-]+)?([[:space:]]|$)'

status=0
for file in "${files[@]}"; do
  n=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    n=$((n + 1))
    [[ "$line" =~ $uses ]] || continue
    ref="${BASH_REMATCH[3]}"
    [[ "$ref" == ./* ]] && continue
    if [[ ! "$ref" =~ $pinned ]]; then
      echo "$file:$n: $ref is not pinned by a full commit SHA; write it as <action>@<40-character SHA> # <version>"
      status=1
    elif [[ ! "${line#*"$ref"}" =~ $version ]]; then
      echo "$file:$n: $ref has no version comment; add the release the SHA belongs to after it: # v1.2.3"
      status=1
    fi
  done < "$file"
done
exit "$status"
