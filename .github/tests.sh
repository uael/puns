#!/bin/sh
# The test files of `puns` that jobs of CI run, one `--test NAME` a line, for `$1`:
# - `default`: the ones that build without the `unstable` feature, which is the default. Not
#   `compile_fail` and `seeds`, which have jobs of their own.
# - `unstable`: the ones about the feature, to run with it: a target that requires it
#   (`required-features` in `puns/Cargo.toml`), or a file with tests under `cfg(feature = ..)`.
# - `tree`: the ones of `tree_borrows_only`, which the jobs of Miri with stacked borrows run with
#   tree borrows apart.
# With `MIRI=1`, only the files that Miri runs: not the ones that start with `#![cfg(not(miri))]`.
# With `STACKED=1` too, only the ones that Miri runs with stacked borrows: not the ones of
# `tree_borrows_only`.
set -eu
# The files that read through references what stacked borrows gives them no access to, and that
# Miri runs with tree borrows only: the safe reads of pointees outside their values.
tree_borrows_only="aim_reads"
cd "$(dirname "$0")/../puns"
if [ "$1" = tree ]; then
  for name in $tree_borrows_only; do echo "--test $name"; done
  exit 0
fi
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
  if [ "${STACKED:-0}" = 1 ]; then
    case " $tree_borrows_only " in
      *" $name "*) continue ;;
    esac
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
      echo "usage: $0 default|unstable|tree" >&2
      exit 2
      ;;
  esac
done
