#!/usr/bin/env bash
set -euo pipefail

config="release-please-config.json"
manifest=".release-please-manifest.json"

[[ "$(jq -r '.packages | keys | join(" ")' "$config")" == "." ]]
[[ "$(jq -r '.packages["."].["release-type"]' "$config")" == "rust" ]]
[[ "$(jq -r '.packages["."].["changelog-path"]' "$config")" == "backend/CHANGELOG.md" ]]
[[ "$(jq -r '.packages["."].["extra-files"][]' "$config")" == "frontend/package.json" ]]
[[ "$(jq -r 'keys | join(" ")' "$manifest")" == "." ]]
[[ -f backend/CHANGELOG.md ]]
[[ -f frontend/package.json ]]

echo "release-please root package configuration is valid"
