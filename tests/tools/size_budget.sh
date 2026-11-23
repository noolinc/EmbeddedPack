#!/usr/bin/env bash
# tools/size_budget.py against a fake `size` tool and a throwaway profile.
set -u
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
W="$(mktemp -d "${TMPDIR:-/tmp}/size-budget-XXXXXX")"; trap 'rm -rf "$W"' EXIT
FAILS=0
cat > "$W/profile.toml" <<'P'
[hardware]
ram_bytes = "1k"
flash_bytes = 2048
[budget]
flash_warn_percent = 80
flash_max_percent = 90
ram_warn_percent = 50
ram_max_percent = 100
P
fake() { # <text> <data> <bss>
  printf '#!/bin/sh\necho "   text    data     bss     dec     hex filename"\necho "   %s   %s   %s   0   0 fw.elf"\n' "$1" "$2" "$3" > "$W/size"; chmod +x "$W/size"
}
case_() { # <name> <want rc> <needle> <text> <data> <bss> [profile]
  fake "$4" "$5" "$6"
  out="$(python3 "$ROOT/tools/size_budget.py" "$W/fw.elf" --profile "${7:-$W/profile.toml}" --size "$W/size" 2>&1)"; rc=$?
  if [ "$rc" = "$2" ] && echo "$out" | grep -qF -- "$3"; then echo "ok   $1"; else echo "FAIL $1 (rc=$rc)"; echo "$out"; FAILS=$((FAILS+1)); fi
}
case_ "under budget" 0 "ok   flash: 1100 of 2048" 1000 100 100
case_ "flash in the warn band" 0 "WARN flash: 1700 of 2048" 1600 100 100
case_ "flash over max" 1 "FAIL flash: 1900 of 2048" 1800 100 100
case_ "RAM in the warn band" 0 "WARN RAM: 600 of 1024" 500 100 500
case_ "RAM over max" 1 "FAIL RAM: 1100 of 1024" 500 100 1000
printf '[hardware]\nram_bytes = 1024\nflash_bytes = 2048\n[budget]\nflash_warn_percent = 95\nflash_max_percent = 90\n' > "$W/bad.toml"
case_ "warn above max is an error" 1 "above max" 1 1 1 "$W/bad.toml"
case_ "missing profile is an error" 1 "error" 1 1 1 "$W/none.toml"
[ "$FAILS" -eq 0 ] && echo "PASS" || { echo "$FAILS failure(s)"; exit 1; }
