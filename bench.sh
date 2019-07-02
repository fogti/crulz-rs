#!/bin/bash

brp() {
  echo "docs/bench_results${1}.txt"
}

cd "$(dirname "$0")"

while read LINE; do
  case "$LINE" in
    (a|ap)
      ( cargo build & cargo build --release & wait ) \
        && git add .
      ;;
    (d|diff|delta)
      git diff
      continue
      ;;
    (q|quit)
      break
      ;;
    (u|use|up)
      mv -T "$(brp 2)" "$(brp)"
      ;;
  esac
  cargo bench > "$(brp 2)" && cargo benchcmp --threshold 4 "$(brp)" "$(brp 2)"
done
