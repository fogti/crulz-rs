#!/bin/bash

SUM=0
for i in $(seq 10000); do
  X="$(target/release/crulz -q docs/stress_example.txt | cut -f2 -d' ')"
  ((SUM+=X))
  echo "$i $X $(echo "$SUM / $i" | bc -l)"
done
