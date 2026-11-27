#!/usr/bin/env python3
"""Send the one vendor request Pioneer's audio driver makes to a DDJ-1000.

Without it the DDJ-1000 jog displays show "NO AUDIO DRIVER" (no Linux driver
sends it). The request is a device-to-host control read of two bytes:
bmRequestType 0xC0, bRequest 0x00, wValue 0x0000, wIndex 0x8003, wLength 2.
It was documented by https://github.com/RudolfsUiska/ddj1000-linux-mixxx
(docs/PROTOCOL.md). This does not provide audio; MIDI works without it.

Run as root, after plugging the controller in (repeat after each replug):
    sudo python3 tools/ddj1000_announce.py
"""

import ctypes
import fcntl
import glob
import os
import sys

USBDEVFS_CONTROL = 0xC0185500  # _IOWR('U', 0, struct usbdevfs_ctrltransfer), 64-bit
VENDOR, PRODUCT = "2b73", "0020"


class CtrlTransfer(ctypes.Structure):
    _fields_ = [
        ("bRequestType", ctypes.c_uint8),
        ("bRequest", ctypes.c_uint8),
        ("wValue", ctypes.c_uint16),
        ("wIndex", ctypes.c_uint16),
        ("wLength", ctypes.c_uint16),
        ("timeout", ctypes.c_uint32),
        ("data", ctypes.c_void_p),
    ]


def read(path):
    with open(path) as f:
        return f.read().strip()


def device_nodes():
    for id_file in glob.glob("/sys/bus/usb/devices/*/idVendor"):
        base = os.path.dirname(id_file)
        if read(id_file) == VENDOR and read(base + "/idProduct") == PRODUCT:
            yield "/dev/bus/usb/%03d/%03d" % (
                int(read(base + "/busnum")),
                int(read(base + "/devnum")),
            )


def main():
    nodes = list(device_nodes())
    if not nodes:
        print("No DDJ-1000 (2b73:0020) found.", file=sys.stderr)
        return 1
    status = 0
    for node in nodes:
        try:
            fd = os.open(node, os.O_RDWR)
        except OSError as error:
            print(f"{node}: {error} (run with sudo)", file=sys.stderr)
            status = 1
            continue
        try:
            buf = ctypes.create_string_buffer(2)
            request = CtrlTransfer(
                0xC0, 0x00, 0x0000, 0x8003, 2, 1000, ctypes.cast(buf, ctypes.c_void_p)
            )
            fcntl.ioctl(fd, USBDEVFS_CONTROL, request)
            print(f"{node}: request accepted, device returned {buf.raw[:2].hex()}")
        except OSError as error:
            print(f"{node}: request failed: {error}", file=sys.stderr)
            status = 1
        finally:
            os.close(fd)
    return status


if __name__ == "__main__":
    sys.exit(main())
