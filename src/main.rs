// SPDX-License-Identifier: GPL-2.0-or-later
//! lsusb: list USB devices.

fn main() -> std::process::ExitCode {
    exausb::cli::run()
}
