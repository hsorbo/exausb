// SPDX-License-Identifier: GPL-2.0-or-later
//! exausb: list USB devices and dump their descriptors on any platform
//! libusb supports.  A cross-platform sibling of usbutils' `lsusb`.
//!
//! Derived from `lsusb.c` and friends from usbutils
//! (Copyright (C) 1999-2001, 2003 Thomas Sailer, Copyright (C) 2003-2005
//! David Brownell, and others; see the individual modules).

pub mod cli;
mod desc_defs;
mod desc_dump;
mod dump;
pub mod model;
mod names;
pub mod render;
mod sysfs;
mod tree;
mod usb;
mod usb_spec;
mod usbmisc;
