#!/bin/bash

cd "$(dirname "$0")"
cargo build --release || exit $?
echo

for i in examples/*; do
  echo "$i"
  cat "$i"
  target/release/crulz "$@" "$i"
  #time target/release/crulz "$@" "$i"
  echo
done
