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
# One exception, in a socle's workflow templates only (a path under a
# `templates/` directory): a reusable workflow of mc-cli called at the release
# tag of this workspace's version, read from Cargo.toml:
#
#   uses: payangar-studio/mc-cli/.github/workflows/publish-mod.yml@v0.4.0
#
# A caller template pins the release that embeds it, and that release's commit
# SHA does not exist yet when the template is written. This repository's own
# workflows call it by its local path.
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
release=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version = "\(.*\)"$/\1/p' "$(dirname "$0")/../Cargo.toml")
[[ -n "$release" ]] || { echo "Cargo.toml: no version under [workspace.package]"; exit 2; }

status=0
for file in "${files[@]}"; do
  n=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    n=$((n + 1))
    [[ "$line" =~ $uses ]] || continue
    ref="${BASH_REMATCH[3]}"
    [[ "$ref" == ./* ]] && continue
    [[ "$file" == */templates/* && "$ref" =~ ^payangar-studio/mc-cli/\.github/workflows/[^/@]+@v(.*)$ && "${BASH_REMATCH[1]}" == "$release" ]] && continue
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
