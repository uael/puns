#!/bin/sh
# The test files of `puns` that jobs of CI run, one `--test NAME` a line, for `$1`:
# - `default`: the ones that build without the `unstable` feature, which is the default. Not
#   `compile_fail` and `seeds`, which have jobs of their own.
# - `unstable`: the ones about the feature, to run with it: a target that requires it
#   (`required-features` in `puns/Cargo.toml`), or a file with tests under `cfg(feature = ..)`.
# With `MIRI=1`, only the files that Miri runs: not the ones that start with `#![cfg(not(miri))]`.
# With `STACKED=1` too, only the ones that Miri runs with stacked borrows: not the ones whose
# documentation says `Miri: tree borrows only`, which read through references what stacked borrows
# gives them no access to.
set -eu
cd "$(dirname "$0")/../puns"
required=$(awk '
  /^\[\[/ { test = ($0 == "[[test]]") }
  test && /^name = / { gsub(/"/, "", $3); name = $3 }
  test && /^required-features = \["unstable"\]/ { print name }
' Cargo.toml)
for file in tests/*.rs; do
  name=$(basename "$file" .rs)
  case "$name" in compile_fail | seeds) continue ;; esac
  if [ "${MIRI:-0}" = 1 ] && grep -q 'cfg(not(miri))' "$file"; then
    continue
  fi
  if [ "${STACKED:-0}" = 1 ] && grep -q '^//! Miri: tree borrows only' "$file"; then
    continue
  fi
  case " $(echo $required) " in
    *" $name "*) needs=yes ;;
    *) needs=no ;;
  esac
  case "$1" in
    default)
      if [ "$needs" = no ]; then echo "--test $name"; fi
      ;;
    unstable)
      if [ "$needs" = yes ] || grep -q 'feature = "unstable"' "$file"; then echo "--test $name"; fi
      ;;
    *)
      echo "usage: $0 default|unstable" >&2
      exit 2
      ;;
  esac
done
