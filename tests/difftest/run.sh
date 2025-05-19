#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
#
# Differential test: dump the same synthetic descriptors with upstream
# usbutils' C code and with this port, and compare the output byte for byte.
#
# Usage: tests/difftest/run.sh [seed] [count]
# Needs: a C compiler, curl, python3 and libusb-1.0 (pkg-config).
set -eu

USBUTILS_COMMIT=24e48ca72b057f86356e9f0f156b858a050cbfc7
SEED=${1:-1}
COUNT=${2:-2000}

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
work="$root/target/difftest"
src="$work/usbutils-$USBUTILS_COMMIT"
mkdir -p "$src"

for f in lsusb.c lsusb.h names.h sysfs.h usbmisc.h desc-defs.c desc-defs.h \
         desc-dump.c desc-dump.h usb-spec.h; do
	[ -s "$src/$f" ] || curl -sfL -o "$src/$f" \
		"https://raw.githubusercontent.com/gregkh/usbutils/$USBUTILS_COMMIT/$f"
done

${CC:-cc} -O1 -w -std=gnu11 -DPACKAGE_NAME='"usbutils"' -DVERSION='"test"' \
	-I"$src" $(pkg-config --cflags libusb-1.0) \
	-o "$work/harness-c" "$here/harness.c" "$src/desc-defs.c" "$src/desc-dump.c" \
	$(pkg-config --libs libusb-1.0)

(cd "$root" && cargo build -q --example difftest)

python3 "$here/gen.py" "$SEED" "$COUNT" > "$work/input.txt"
"$work/harness-c" < "$work/input.txt" > "$work/out-c.txt" 2> "$work/err-c.txt"
USB_IDS=/nonexistent "$root/target/debug/examples/difftest" < "$work/input.txt" \
	> "$work/out-rust.txt" 2> "$work/err-rust.txt"

if ! cmp -s "$work/err-c.txt" "$work/err-rust.txt"; then
	echo "STDERR MISMATCH (seed $SEED); first differences:"
	diff "$work/err-c.txt" "$work/err-rust.txt" | head -20
	exit 1
fi
if cmp -s "$work/out-c.txt" "$work/out-rust.txt"; then
	echo "OK: $COUNT records, seed $SEED, $(wc -l < "$work/out-c.txt") identical output lines"
else
	echo "MISMATCH (seed $SEED); first differences:"
	diff "$work/out-c.txt" "$work/out-rust.txt" | head -40
	exit 1
fi
