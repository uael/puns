#!/bin/sh
# The test files of `puns-model` that the jobs of `model.yml` run, for `$1`:
# - `groups`: the names of the groups, as a JSON array, for the matrix of the workflow.
# - a group: its files, one `--test NAME` a line. The last group, `rest`, takes `--lib` and every
#   file of `tests/` that no other group names, so that a new file runs.
# A group is the files that one job builds. One `rustc` builds each file, and up to four run at
# once on a runner (4 CPUs, 16 GB). The largest files (`size2_*`, `size1_*`) each need up to about
# 3.5 GiB, so a group has at most three of the largest, and the groups are chosen by the time that
# their files take to build, so that the jobs take about as long.
set -eu
cd "$(dirname "$0")/../puns-model"
groups='
1 size2_1 size2_2 size1_1
2 size2_3 size2_4 size1_2
3 size2_5 size1_3 parts3
4 parts4_1 parts4_2 size4_1 size3
5 parts2_1 parts2_2 size4_2 parts8
'
named=$(echo "$groups" | awk 'NF { for (i = 2; i <= NF; i++) print $i }')
twice=$(echo "$named" | sort | uniq -d)
if [ -n "$twice" ]; then
  echo "in two groups: $twice" >&2
  exit 1
fi
case "$1" in
  groups)
    echo "[$(echo "$groups" | awk 'NF { printf "\"%s\", ", $1 }')\"rest\"]"
    ;;
  rest)
    echo "--lib"
    for file in tests/*.rs; do
      name=$(basename "$file" .rs)
      case " $(echo $named) " in
        *" $name "*) ;;
        *) echo "--test $name" ;;
      esac
    done
    ;;
  *)
    files=$(echo "$groups" | awk -v group="$1" '$1 == group { for (i = 2; i <= NF; i++) print $i }')
    if [ -z "$files" ]; then
      echo "no group $1" >&2
      exit 2
    fi
    for name in $files; do
      # A named file that is not there fails the job rather than run nothing.
      if [ ! -f "tests/$name.rs" ]; then
        echo "no file tests/$name.rs" >&2
        exit 1
      fi
      echo "--test $name"
    done
    ;;
esac
