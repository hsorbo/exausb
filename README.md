# exausb

A Rust port of `lsusb` from [usbutils](https://github.com/gregkh/usbutils)
(ported from commit `24e48ca7`), built on libusb through
[rusb](https://crates.io/crates/rusb).  It runs on Linux and macOS (and
anywhere else libusb does).

```
cargo build --release
./target/release/lsusb            # list devices
./target/release/lsusb -v         # dump all descriptors
./target/release/lsusb -t         # show the device tree
```

All of upstream's options are supported: `-v`, `-s [[bus]:][devnum]`,
`-d vendor:[product]`, `-D device`, `-t`, `-V` and `-h`.

## Vendor and product names

Upstream gets vendor, product and class names from the udev hwdb, which only
exists on Linux.  This port embeds a copy of `usb.ids` from the
[Linux USB ID Repository](http://www.linux-usb.org/usb-ids.html) in the
binary (`data/usb.ids`), so names work everywhere without setup.

To refresh the bundled copy, run the script and rebuild:

```
scripts/update-usb-ids.sh
cargo build --release
```

To use a different file without rebuilding, set `USB_IDS=/path/to/usb.ids`.

`usb.ids` is maintained by Stephen J. Gowdy and contributors; see the header
of `data/usb.ids`.

## Differences from upstream

- **Name database:** names come from the bundled `usb.ids` (see above)
  rather than the udev hwdb.
- **Device strings without sysfs:** where there's no sysfs (non-Linux),
  manufacturer, product and serial strings are read from the device's string
  descriptors.
- **`lsusb -t` without sysfs:** the tree is built from libusb's topology.
  Kernel drivers aren't known then, so they are shown as `[unknown]`.  Busses
  whose root hub libusb doesn't report (macOS) get a placeholder node.
- **Malformed descriptors:** upstream occasionally reads a byte past the end
  of a descriptor (for example `bmVideoStandards` in a UVC 1.0 processing
  unit, or `bmaControls` in a UVC header with `bControlSize` 0).  It then
  prints whatever byte follows in memory; this port prints 0.
- **Error messages:** messages that upstream builds from `errno` (e.g.
  "can't get device qualifier") show the libusb error instead.
- **Version:** `-V` reports this crate's name and version.

## Testing

`cargo test` runs the unit tests.

`tests/difftest/run.sh [seed] [count]` is a differential test against the
original C code.  It downloads upstream's sources at the ported commit and
builds them into a small harness (`tests/difftest/harness.c`), with udev and
sysfs stubbed out.  It then feeds randomly generated configuration, interface,
endpoint, hub, HID report and BOS descriptors to both implementations, and
compares stdout and stderr byte for byte.  It needs a C compiler, curl,
python3 and libusb-1.0 (found with pkg-config).

## License

GPL-2.0-or-later, like usbutils.  See `LICENSE`.
