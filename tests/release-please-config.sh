#!/usr/bin/env bash
set -euo pipefail

config="release-please-config.json"
manifest=".release-please-manifest.json"

[[ "$(jq -r '.packages | keys | join(" ")' "$config")" == "." ]]
[[ "$(jq -r '.packages["."].["release-type"]' "$config")" == "go" ]]
[[ "$(jq -r '.packages["."].["changelog-path"]' "$config")" == "backend/CHANGELOG.md" ]]
[[ "$(jq -r '.packages["."].["extra-files"][] | [.type, .path, (.jsonpath // "")] | @tsv' "$config")" == $'toml\tbackend/Cargo.toml\t$.package.version\ntoml\tbackend/Cargo.lock\t$.package[?(@.name=="oxide-player")].version\njson\tfrontend/package.json\t$.version' ]]
[[ "$(jq -r 'keys | join(" ")' "$manifest")" == "." ]]
[[ -f backend/CHANGELOG.md ]]
[[ -f frontend/package.json ]]

echo "release-please root package configuration is valid"
