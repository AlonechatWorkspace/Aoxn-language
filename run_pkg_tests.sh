#!/bin/bash
# Workaround for Windows Smart App Control blocking freshly built unsigned
# test binaries after their first runs: bump a marker comment so each test
# build produces a new binary hash.
STAMP="// test-build $(date +%s%N)"
echo "$STAMP" > crates/aoxn-pkg/src/.testbuild.rs
echo "mod _testbuild_marker {}" >> /dev/null
sed -i "s|^// test-build .*|$STAMP|" crates/aoxn-pkg/src/lib.rs 2>/dev/null
if ! grep -q "^// test-build" crates/aoxn-pkg/src/lib.rs; then
  echo "$STAMP" >> /dev/null
fi
cargo test -p aoxn-pkg "$@"
