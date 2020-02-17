#!/bin/sh

if ! [ -x "$(which jq)" ]; then
  echo "run_examples.sh: jq not found!"
  exit 1
fi

cd "$(dirname "$0")"
cargo build --release || exit $?
echo

CRULZ="$(cargo metadata --format-version 1 | jq -r '.target_directory')/release/crulz"
if ! [ -x "$CRULZ" ]; then
  echo "run_examples.sh: crulz not found!"
  exit 1
fi

for i in examples/*; do
  echo "$i"
  cat "$i"
  "$CRULZ" "$@" "$i"
  #time target/release/crulz "$@" "$i"
  echo
done
