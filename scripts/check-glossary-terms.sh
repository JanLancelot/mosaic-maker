#!/bin/sh
# Fails if Rust code, AGENTS.md or CODING_STANDARDS.md uses a term GLOSSARY.md
# lists under _Avoid_. Matches identifiers, comments and strings,
# case-insensitively; a space in a term also matches `_`, `-` or nothing, so
# "tile image" catches tile_image and TileImage.
root=$(cd "$(dirname "$0")/.." && pwd)
tab=$(printf '\t')

# Avoided terms with an unrelated, legitimate meaning in code, separated by `|`.
#   result: Rust's Result type.
#   hash: Source Image identity is a content hash (ADR 0003).
#   average color: a Color Signature is built from the average colors of its parts.
exempt='result|hash|average color'

# Code we don't name that happens to contain an avoided term, as one extended
# regex. It is removed from a matching line before the line is checked again.
#   Command::output: runs a process and captures what it printed (std).
#   FileDialog::save_file: asks where to save a file (rfd).
exempt_code='Command::output|\.output\(\)|\.save_file\(\)'

# One "avoided<TAB>Glossary Term" line per avoided term.
pairs=$(awk '
    /^\*\*.*\*\*:/ { term = $0; gsub(/\*\*|:/, "", term) }
    /^_Avoid_:/ {
        sub(/^_Avoid_: */, "")
        n = split($0, avoided, / *, */)
        for (i = 1; i <= n; i++) print tolower(avoided[i]) "\t" term
    }' "$root/GLOSSARY.md")

status=0
while IFS="$tab" read -r avoided term; do
    case "|$exempt|" in *"|$avoided|"*) continue ;; esac
    word=$(printf '%s' "$avoided" | sed 's/ /[ _-]?/g')
    pattern="(^|[^a-z0-9])${word}s?([^a-z0-9]|$)"
    matches=$(
        {
            grep -rniE "$pattern" "$root/crates" --include='*.rs'
            grep -HniE "$pattern" "$root/AGENTS.md" "$root/CODING_STANDARDS.md"
        } | while IFS= read -r line; do
            if printf '%s\n' "$line" | sed -E "s/$exempt_code//g" | grep -qiE "$pattern"; then
                printf '%s\n' "$line"
            fi
        done
    )
    if [ -n "$matches" ]; then
        printf '%s\n' "$matches"
        echo "Use the glossary term \"$term\" instead of \"$avoided\" (GLOSSARY.md)." >&2
        status=1
    fi
done <<PAIRS
$pairs
PAIRS
exit $status
