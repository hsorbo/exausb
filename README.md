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
exists on Linux.  This port reads the `usb.ids` file directly.  It looks in
the usual places (`/usr/share/hwdata/usb.ids`, `/usr/share/misc/usb.ids`, the
Homebrew and MacPorts prefixes, ...), or in the file named by the `USB_IDS`
environment variable.  On macOS, get one with e.g.

```
curl -o ~/usb.ids https://raw.githubusercontent.com/usbids/usbids/master/usb.ids
export USB_IDS=~/usb.ids
```

Without it, names fall back to the device's own manufacturer and product
strings, as in upstream.

## Differences from upstream

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
