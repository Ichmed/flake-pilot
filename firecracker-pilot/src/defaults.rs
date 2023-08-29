//
// Copyright (c) 2022 Elektrobit Automotive GmbH
//
// This file is part of flake-pilot
//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
//
use std::{env, path::Path};

use flakes::paths;

paths!(
    IMAGE_ROOT = "image";
    IMAGE_OVERLAY = "overlayroot";
    OVERLAY_ROOT = "overlayroot/rootfs";
    OVERLAY_UPPER = "overlayroot/rootfs_upper";
    OVERLAY_WORK = "overlayroot/rootfs_work";
    FIRECRACKER_OVERLAY_DIR = "/var/lib/firecracker/storage";
    FIRECRACKER_TEMPLATE = "/etc/flakes/firecracker.json";
    FIRECRACKER_FLAKE_DIR = "/usr/share/flakes";
    FIRECRACKER_VMID_DIR = "/var/lib/firecracker/storage/tmp/flakes";
    SOCAT = "/usr/bin/socat";
);

pub const GC_THRESHOLD: usize = 20;
pub const VM_CID: u32 = 3;
pub const VM_PORT: u32 = 52;
pub const RETRIES: u32 = 60;
pub const VM_WAIT_TIMEOUT_MSEC: u64 = 1000;

pub fn is_debug() -> bool {
    env::var("PILOT_DEBUG").is_ok()
}

pub fn debug(message: &str) {
    if is_debug() {
        debug!("{}", message)
    }
}
