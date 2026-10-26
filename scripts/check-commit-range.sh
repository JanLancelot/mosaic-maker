#!/bin/sh
# Usage: check-commit-range.sh <revision range>
# Checks the subject of every commit in the range. Root commits predate the
# convention and are exempt.
set -e
dir=$(dirname "$0")
status=0
for commit in $(git rev-list --min-parents=1 "$1"); do
    "$dir/check-commit-subject.sh" "$(git log -n 1 --format=%s "$commit")" || status=1
done
exit $status
