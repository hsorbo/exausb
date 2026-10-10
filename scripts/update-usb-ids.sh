#!/bin/sh
# SPDX-License-Identifier: GPL-2.0-or-later
#
# Refresh the bundled data/usb.ids from the Linux USB ID Repository
# (http://www.linux-usb.org/usb-ids.html).  Rebuild afterwards to embed it.
set -eu

URLS="http://www.linux-usb.org/usb.ids
https://raw.githubusercontent.com/usbids/usbids/master/usb.ids"

root=$(cd "$(dirname "$0")/.." && pwd)
dest="$root/data/usb.ids"
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

ok=
for url in $URLS; do
	echo "Fetching $url"
	if curl -fsSL --retry 2 -o "$tmp" "$url"; then
		ok=1
		break
	fi
	echo "  failed" >&2
done
[ -n "$ok" ] || { echo "Could not download usb.ids" >&2; exit 1; }

# Sanity check: a truncated download or an HTML error page must not replace
# a good copy.  (The file is mostly UTF-8 with a few stray Latin-1 bytes; it
# is embedded as bytes and decoded leniently, so that's fine.)
if ! grep -q '^# Version:' "$tmp" || ! grep -q '^1d6b  Linux Foundation' "$tmp" \
	|| ! grep -q '^C 09  Hub' "$tmp"; then
	echo "Downloaded file doesn't look like usb.ids" >&2
	exit 1
fi

old=$(grep -m1 '^# Version:' "$dest" 2>/dev/null || echo "(none)")
new=$(grep -m1 '^# Version:' "$tmp")
mkdir -p "$(dirname "$dest")"
cp "$tmp" "$dest"
echo "data/usb.ids: ${old#\# } -> ${new#\# }"
