#!/bin/zsh
# smoke.sh <seconds> <logfile> <cmd...>  — run cmd for N seconds, then verify
# the log contains REAL app output (fail on vacuous logs), grep for issues.
secs=$1; log=$2; shift 2
perl -e 'alarm shift @ARGV; exec @ARGV' "$secs" "$@" > "$log" 2>&1
lines=$(wc -l < "$log")
issues=$(grep -icE "panic|WARN|ERROR" "$log")
if ! grep -qE "bevy|wgpu|Adapter|INFO" "$log"; then
  echo "VACUOUS LOG ($lines lines) — app did not run"; head -3 "$log"; exit 2
fi
echo "ran OK, $issues issues"
[ "$issues" != "0" ] && grep -iE "panic|WARN|ERROR" "$log" | sort | uniq -c | sort -rn | head -8
exit 0
