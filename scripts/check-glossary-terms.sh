#!/bin/sh
# Fails if Rust code uses a term GLOSSARY.md lists under _Avoid_. Matches
# identifiers, comments and strings, case-insensitively; a space in a term also
# matches `_`, `-` or nothing, so "tile image" catches tile_image and TileImage.
root=$(cd "$(dirname "$0")/.." && pwd)
tab=$(printf '\t')

# Avoided terms with an unrelated, legitimate meaning in code.
#   result: Rust's Result type.
exempt='result'

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
    case " $exempt " in *" $avoided "*) continue ;; esac
    word=$(printf '%s' "$avoided" | sed 's/ /[ _-]?/g')
    if grep -rniE "(^|[^a-z0-9])${word}s?([^a-z0-9]|$)" "$root/crates" --include='*.rs'; then
        echo "Use the glossary term \"$term\" instead of \"$avoided\" (GLOSSARY.md)." >&2
        status=1
    fi
done <<PAIRS
$pairs
PAIRS
exit $status
