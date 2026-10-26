#!/bin/sh
# Usage: check-commit-subject.sh <subject>
# Fails unless the subject has a Conventional Commits header (see AGENTS.md).
# Merge commits are exempt.
subject="$1"
types='feat|fix|docs|test|refactor|chore|ci|build|perf'

case "$subject" in
Merge\ *) exit 0 ;;
esac

if printf '%s\n' "$subject" | grep -Eq "^($types): [^ ]"; then
    exit 0
fi

echo "Commit subject lacks a Conventional Commits header: \"$subject\"" >&2
echo "Expected 'type: summary' with type one of: $(echo "$types" | sed 's/|/, /g')." >&2
exit 1
