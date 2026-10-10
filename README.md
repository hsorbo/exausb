# exausb

List USB devices and dump their descriptors, on any platform libusb
supports (Linux, macOS, ...).

exausb is a cross-platform sibling of `lsusb` from
[usbutils](https://github.com/gregkh/usbutils).  It takes the same options
and prints descriptors in the same format, so if you know `lsusb` you know
exausb.  It's written in Rust on top of libusb (through
[rusb](https://crates.io/crates/rusb)), and doesn't depend on anything
Linux-specific like udev or sysfs (though it uses sysfs where available).

```
cargo build --release
./target/release/exausb            # list devices
./target/release/exausb -v         # dump all descriptors
./target/release/exausb -t         # show the device tree
```

Options: `-v` (repeat for more), `-s [[bus]:][devnum]`,
`-d vendor:[product]`, `-D device`, `-t`, `-V` and `-h`, as in `lsusb`.

## Vendor and product names

A copy of `usb.ids` from the
[Linux USB ID Repository](http://www.linux-usb.org/usb-ids.html) is embedded
in the binary (`data/usb.ids`), so names work everywhere without setup.

To refresh the bundled copy, run the script and rebuild:

```
scripts/update-usb-ids.sh
cargo build --release
```

To use a different file without rebuilding, set `USB_IDS=/path/to/usb.ids`.

`usb.ids` is maintained by Stephen J. Gowdy and contributors; see the header
of `data/usb.ids`.

## Compared to lsusb

- **Verbose output (`-v`)** is the same as `lsusb -v`, apart from the
  details below.
- **Names** come from the bundled `usb.ids` rather than the udev hwdb.
- **Device strings:** without sysfs (non-Linux), manufacturer, product and
  serial strings are read from the device itself.
- **The tree (`-t`)** shows only what is known on every platform: no kernel
  driver names, port counts only for hubs whose descriptor could be read,
  and sysfs paths (`-vv`) only where sysfs exists.  On Linux it's read from
  sysfs, elsewhere from libusb's topology; on macOS, libusb doesn't report
  root hubs, so a bus line shows just the bus number:
  ```
  /:  Bus 002
      |__ Port 001: Dev 001, If 0, Class=Imaging, 480M
  ```
- **Malformed descriptors:** lsusb occasionally reads a byte past the end of
  a descriptor (for example `bmVideoStandards` in a UVC 1.0 processing unit,
  or `bmaControls` in a UVC header with `bControlSize` 0) and prints whatever
  byte follows in memory; exausb prints 0.
- **Error messages** that lsusb builds from `errno` (e.g. "can't get device
  qualifier") show the libusb error instead.

## Testing

`cargo test` runs the unit tests.

`tests/difftest/run.sh [seed] [count]` checks that descriptor dumping matches
lsusb.  It downloads usbutils' sources (at commit `24e48ca7`, which exausb
derives from) and builds their descriptor dumping into a small harness
(`tests/difftest/harness.c`), with udev and sysfs stubbed out.  It then feeds
randomly generated configuration, interface, endpoint, hub, HID report and
BOS descriptors to both, and compares stdout and stderr byte for byte.  It
needs a C compiler, curl, python3 and libusb-1.0 (found with pkg-config).

## License

GPL-2.0-or-later.  exausb is derived from usbutils, and keeps its license and
copyright notices.  See `LICENSE`.
