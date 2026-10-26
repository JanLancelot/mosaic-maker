#!/bin/sh
# Usage: check-branch-name.sh <branch>
# Fails unless the branch is named `type/short-summary` (see AGENTS.md).
branch="$1"
types=$(cat "$(dirname "$0")/conventional-types")

if printf '%s\n' "$branch" | grep -Eq "^($types)/[a-z0-9][a-z0-9-]*$"; then
    exit 0
fi

echo "Branch name doesn't follow 'type/short-summary': \"$branch\"" >&2
echo "Expected type one of: $(echo "$types" | sed 's/|/, /g'); summary in lowercase-kebab-case." >&2
exit 1
