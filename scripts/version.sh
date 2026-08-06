#!/usr/bin/env bash
#
# Version arithmetic for the release workflow.
#
# main's Cargo.toml always carries the version the *next* release will cut.
# The release workflow reads it with `get`, tags that version, and only once
# the release is published does it `set` the `next` one — so no commit on main
# ever shares a version number with a published build.
#
#   version.sh get [dir]          print the [package] version in dir/Cargo.toml
#   version.sh next <x.y.z>       print the next minor version (patch zeroed)
#   version.sh set <x.y.z> [dir]  write the version to Cargo.toml and Cargo.lock
#
# dir defaults to the repository root.

set -euo pipefail

readonly ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

die() {
  echo "version.sh: $*" >&2
  exit 1
}

require_semver() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || die "not a x.y.z version: '$1'"
}

# The version in the [package] table — not a [dependencies] entry, and not the
# [workspace] table that sits above it.
package_field() {
  local manifest="$1" field="$2"
  awk -v field="$field" '
    /^\[package\]/ { in_pkg = 1; next }
    /^\[/          { in_pkg = 0 }
    in_pkg && $0 ~ "^" field "[[:space:]]*=" {
      match($0, /"[^"]*"/)
      print substr($0, RSTART + 1, RLENGTH - 2)
      exit
    }
  ' "$manifest"
}

cmd_get() {
  local dir="${1:-$ROOT}" manifest
  manifest="$dir/Cargo.toml"
  [[ -f "$manifest" ]] || die "no manifest at $manifest"
  local version
  version="$(package_field "$manifest" version)"
  [[ -n "$version" ]] || die "no [package] version in $manifest"
  echo "$version"
}

cmd_next() {
  local version="${1-}"
  require_semver "$version"
  local major minor
  IFS='.' read -r major minor _ <<<"$version"
  echo "${major}.$((minor + 1)).0"
}

cmd_set() {
  local version="${1-}" dir="${2:-$ROOT}"
  require_semver "$version"
  local manifest="$dir/Cargo.toml" lock="$dir/Cargo.lock"
  [[ -f "$manifest" ]] || die "no manifest at $manifest"

  local name
  name="$(package_field "$manifest" name)"
  [[ -n "$name" ]] || die "no [package] name in $manifest"

  awk -v new="$version" '
    /^\[package\]/ { in_pkg = 1; print; next }
    /^\[/          { in_pkg = 0 }
    in_pkg && !done && /^version[[:space:]]*=/ {
      print "version = \"" new "\""
      done = 1
      next
    }
    { print }
  ' "$manifest" >"$manifest.tmp"
  mv "$manifest.tmp" "$manifest"

  # Keep the lock's own entry for this crate in step, so the release build
  # does not have to re-resolve the graph just to record a version.
  if [[ -f "$lock" ]]; then
    awk -v name="$name" -v new="$version" '
      hit && /^version[[:space:]]*=/ { print "version = \"" new "\""; hit = 0; next }
      $0 == "name = \"" name "\""    { hit = 1 }
      { print }
    ' "$lock" >"$lock.tmp"
    mv "$lock.tmp" "$lock"
  fi
}

case "${1-}" in
  get)  shift; cmd_get "$@" ;;
  next) shift; cmd_next "$@" ;;
  set)  shift; cmd_set "$@" ;;
  *)    die "usage: version.sh {get [dir]|next <x.y.z>|set <x.y.z> [dir]}" ;;
esac
