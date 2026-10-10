# exausb

List USB devices and decode their descriptors, on any platform libusb
supports (Linux, macOS, ...).  A modern, cross-platform sibling of `lsusb`
from [usbutils](https://github.com/gregkh/usbutils), written in Rust on top
of libusb (through [rusb](https://crates.io/crates/rusb)).

```
$ exausb
BUS  DEV  ID         SPEED  VENDOR       PRODUCT
002  001  05ac:12a8  480M   Apple, Inc.  iPhone 5/5C/5S/6/SE/7/8/X/XR

$ exausb -t
Bus 002
└─ Port 1  Apple, Inc. iPhone 5/5C/5S/6/SE/7/8/X/XR  05ac:12a8  480M · Device 001
   ├─ If 0  Imaging · Still Image Capture · Picture Transfer Protocol (PIMA 15470)
   ├─ If 1  Vendor Specific Class
   └─ If 3  Communications

$ exausb -v -s 2:1
Bus 002 Device 001  Apple, Inc. iPhone 5/5C/5S/6/SE/7/8/X/XR  05ac:12a8  High Speed (480Mbps)
├─ Device Descriptor
│  bcdUSB              2.10
│  bDeviceClass        0       (Defined at Interface level)
│  idVendor            0x05ac  Apple, Inc.
│  ...
├─ Configuration 1  PTP · 500mA · Self Powered
│  ...
│  └─ Interface 0  Imaging · Still Image Capture · Picture Transfer Protocol (PIMA 15470) · PTP
│     ├─ Endpoint 2 OUT  Bulk · 512 bytes
│     ...
```

Output is colored on a terminal and plain when piped; use
`--color auto|always|never` or set `NO_COLOR` to choose.

## Usage

```
cargo build --release
./target/release/exausb [OPTIONS]
```

| Option | |
|---|---|
| `-v`, `--verbose` | Decode all descriptors of each device |
| `-t`, `--tree` | Show the physical USB device hierarchy |
| `-i`, `--interactive` | Browse devices interactively (see below) |
| `-s [[BUS]:][DEVNUM]` | Only devices with this bus and/or device number |
| `-d VENDOR[:PRODUCT]` | Only devices with this vendor and/or product ID (hex) |
| `--color WHEN` | `auto` (default), `always` or `never` |

Like `lsusb`, exausb exits with status 1 when no device matches.

`-v` decodes standard descriptors (device, configuration, interface,
endpoint, BOS and device capabilities, hub, OTG, debug, device qualifier and
status) and class-specific ones: audio (UAC1/2/3) and MIDI, video (UVC), HID
(including report descriptors, where the OS lets us read them), CDC, CCID,
printers (IPP over USB), DFU and wireless.  Some information needs to open
the device; if that's not allowed, exausb shows what it can and says so.

## Interactive mode

`exausb -i` shows the device tree on the left and the selected device's
descriptors on the right, as a foldable tree that starts out showing just
the section summaries.  The device list refreshes every second, so plugging
and unplugging devices shows up live.

| Key | |
|---|---|
| `↑` `↓` / `j` `k`, `PgUp` `PgDn`, `g` `G` | Move |
| `Tab`, `→` / `←` | Switch between the device list and the details |
| `Enter` / `Space`, `→` / `←` | Expand / fold the section under the cursor |
| `e` / `c` | Expand / collapse all sections |
| `r` | Re-read the devices |
| `q` / `Esc` | Quit |

## Vendor and product names

A copy of `usb.ids` from the
[Linux USB ID Repository](http://www.linux-usb.org/usb-ids.html) is embedded
in the binary (`data/usb.ids`), so names work everywhere without setup.
To refresh it, run the script and rebuild:

```
scripts/update-usb-ids.sh
cargo build --release
```

To use a different file without rebuilding, set `USB_IDS=/path/to/usb.ids`.
`usb.ids` is maintained by Stephen J. Gowdy and contributors; see the header
of `data/usb.ids`.

Without a name in the database, exausb shows the device's own manufacturer
and product strings (on Linux read from sysfs, elsewhere from the device).

## Design

Everything is built as a tree of sections, fields (`name  value  note`) and
warnings (`src/model.rs`) and then drawn by one renderer (`src/render.rs`),
which both the command line output and the interactive mode (`src/tui.rs`)
use.
Descriptor decoding lives in `src/dump/` and `src/desc_dump.rs`.

## License

GPL-2.0-or-later.  The descriptor decoding is derived from usbutils and
keeps its copyright notices.  See `LICENSE`.
