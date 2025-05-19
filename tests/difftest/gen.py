#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
"""Generate random synthetic USB descriptors for the differential test.

Output format is documented in examples/difftest.rs.  Byte values are biased
towards small numbers so that count/length fields frequently line up with the
descriptor length and the "valid" code paths get exercised, not just the
"too short" ones.

Usage: gen.py SEED COUNT
"""
import random
import sys

rng = random.Random(int(sys.argv[1]))
COUNT = int(sys.argv[2])


def byte():
    r = rng.random()
    if r < 0.45:
        return rng.randrange(0, 4)
    if r < 0.6:
        return rng.randrange(4, 16)
    return rng.randrange(0, 256)


def body(n):
    return [byte() for _ in range(n)]


def hexs(bs):
    return "".join("%02x" % b for b in bs) if bs else "-"


def desc(dtype, subtype=None, length=None):
    """A descriptor with bLength set to its real length (usually)."""
    if length is None:
        length = rng.choice([rng.randrange(2, 16), rng.randrange(3, 40), rng.randrange(3, 80)])
    length = max(length, 2)
    d = [length, dtype] + body(length - 2)
    if subtype is not None and length > 2:
        d[2] = subtype
    return d


def maybe_corrupt(blob):
    r = rng.random()
    if r < 0.03 and blob:
        blob[0] = rng.randrange(0, 2)  # bLength too small
    elif r < 0.06 and blob:
        blob[0] = min(255, len(blob) + rng.randrange(1, 10))  # overruns
    elif r < 0.08:
        blob.append(rng.randrange(0, 256))  # trailing single byte
    return blob


# Interface classes worth exercising: (class, subclass choices, protocol choices)
CLASSES = [
    (0x01, [1, 2, 3, 4], [0x00, 0x20, 0x30, 0x10]),  # audio
    (0x0e, [1, 2, 3], [0x00, 0x01]),                 # video
    (0x02, [2, 6, 0x0d, 0x0e], [0, 1]),              # comm
    (0x0a, [0], [0]),                                # cdc data
    (0x03, [0, 1], [0, 1, 2]),                       # hid
    (0x07, [1], [2, 4]),                             # printer (4 = IPP)
    (0x0b, [0], [0]),                                # ccid
    (0xfe, [1, 2], [1, 2]),                          # application (dfu)
    (0x08, [6], [0x50, 0x62]),                       # mass storage
    (0xe0, [1, 2], [1, 2]),                          # wireless
    (0xff, [0, 0x42], [0, 1]),                       # vendor
]


def cs(subtype, fields):
    """Class-specific interface descriptor with a correct bLength."""
    d = [0, 0x24, subtype] + fields
    d[0] = len(d)
    return d


def audio_template(sub, proto):
    """Well-formed descriptors for paths random bytes rarely reach."""
    if sub == 1:
        p = rng.choice([0, 1, 2])
        ptype = rng.choice([1, 2, 3, 4, 7])
        nmodes = rng.choice([0, 1, 3])
        if proto == 0x00:  # UAC1 processing unit (subtype 7)
            n = rng.choice([0, 1, 2])
            ext = [nmodes] + body(2 * nmodes)
            return cs(7, [byte(), ptype, 0, p] + body(p) + [byte()] + body(2) + [0, n] + body(n) + [0] + ext)
        if proto == 0x20:  # UAC2 processing unit (subtype 8)
            ext = [nmodes] + body(4 * nmodes)
            return cs(8, [byte(), ptype, 0, p] + body(p) + [byte()] + body(4) + [0] + body(2) + [0] + ext)
        # UAC3 processing unit (subtype 9)
        ext = rng.choice([[byte()] + body(2) + [nmodes] + body(2 * nmodes), body(rng.randrange(0, 6))])
        return cs(9, [byte(), ptype, 0, p] + body(p) + body(2) + ext)
    if sub == 2:
        r = rng.random()
        if r < 0.4:  # AC-3 / MPEG format specific
            tag = rng.choice([0x1001, 0x1002])
            return cs(3, [tag & 0xff, tag >> 8] + body(5 if tag == 0x1002 else 3) + body(rng.choice([0, 0, 2])))
        # UAC1 format type I/III, continuous or discrete
        n = rng.choice([0, 1, 4])
        return cs(2, [rng.choice([1, 3]), 2, 2, 16, n] + body(6 if n == 0 else 3 * n))
    # MIDI element
    p = rng.choice([0, 1, 2])
    caps = rng.choice([0, 1, 2])
    return cs(4, [byte(), p] + body(2 * p) + [1, byte(), byte(), caps] + body(caps) + [0])


def iface_extra(cls, sub, proto):
    out = []
    for _ in range(rng.choice([0, 1, 1, 2, 3, 5])):
        r = rng.random()
        if cls == 0x0e and r < 0.7:
            st = rng.choice([1, 2, 3, 4, 5, 6, 7, 0x0a, 0x0d, 0x10, 0x11, 0x12, 0x20])
            if st in (5, 7, 0x11) and rng.random() < 0.5:
                # frame descriptors with a consistent interval count
                n = rng.choice([0, 1, 2, 3])
                base = 21 if st == 0x11 else 25
                d = [0] * (26 + 4 * n if n else 38)
                d = d[:2] + body(len(d) - 2)
                d[0], d[1], d[2], d[base] = len(d), 0x24, st, n
                out += d
                continue
            out += desc(0x24, st)
        elif cls == 0x01 and r < 0.25:
            out += audio_template(sub, proto)
        elif cls == 0x01 and r < 0.7:
            st = rng.choice(range(0, 0x12))
            if sub == 2 and st == 2 and rng.random() < 0.5:
                n = rng.choice([0, 1, 2, 9])
                ft = rng.choice([1, 2, 3, 4, 5])
                d = [0, 0x24, 2, ft] + body(4) + [n] + body(3 * n if n else 6) + body(rng.choice([0, 0, 2]))
                d[0] = len(d)
                if ft == 1 or ft == 3:
                    d[7] = n
                out += d
                continue
            if sub == 2 and st == 3:
                d = desc(0x24, st, rng.choice([5, 8, 10, 12]))
                if len(d) > 4:
                    d[3], d[4] = rng.choice([(1, 0x10), (2, 0x10), (1, 0), (5, 0), (6, 0x20), (0, 0x10)])
                out += d
                continue
            out += desc(0x24, st)
        elif cls == 0x02 and r < 0.7:
            st = rng.choice([0, 1, 2, 6, 7, 8, 0x0a, 0x0f, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x1a, 0x1b, 0x1c, 0x30])
            good = {0: 5, 1: 5, 2: 4, 6: 6, 7: 8, 8: 4, 0x0a: 7, 0x0f: 13, 0x11: 5, 0x12: 21,
                    0x13: 9, 0x14: 7, 0x15: 5, 0x16: 22, 0x1a: 6, 0x1b: 12, 0x1c: 8}
            length = good.get(st) if rng.random() < 0.7 else None
            out += desc(0x24, st, length)
        elif cls == 0x03 and r < 0.7:
            n = rng.choice([0, 1, 2])
            d = [6 + 3 * n, 0x21] + body(4 + 3 * n)
            d[5] = n
            for i in range(n):
                d[6 + 3 * i] = rng.choice([0x22, 0x23, 0x21])
            out += d
        elif cls == 0x0b and r < 0.7:
            out += desc(0x21, None, rng.choice([54, 54, 56, 30]))
        elif cls == 0x07 and r < 0.7:
            caps = []
            for _ in range(rng.choice([0, 1, 2])):
                t = rng.choice([0, 0, 1])
                ln = 4 if t == 0 else rng.randrange(0, 4)
                caps += [t, ln] + body(ln)
            d = [4 + len(caps), 0x21, 1, len(caps) and rng.choice([1, 2, 3])] + caps
            out += d
        elif cls == 0xfe and r < 0.7:
            out += desc(0x21, None, rng.choice([7, 9, 5]))
        elif cls == 0xe0 and r < 0.7:
            out += desc(rng.choice([0x21, 0x22, 0x23]), None, rng.choice([4, 14, 3]))
        else:
            dtype = rng.choice([0x0b, 0x09, 0x0c, 0x0e, 0x25, 0x21, 0x24, 0x30, 0x41, 0x05])
            ln = {0x0b: 8, 0x09: 3}.get(dtype) if rng.random() < 0.6 else None
            out += desc(dtype, None, ln)
    if cls == 0x0e:
        out = avoid_uvc_overreads(out)
    return maybe_corrupt(out)


def avoid_uvc_overreads(blob):
    """Upstream reads one byte past some VideoControl/VideoStreaming
    descriptors: it sees the next descriptor's first byte, the port sees 0.
    Lengthen those descriptors by a byte so the comparison stays meaningful.

    - PROCESSING_UNIT of exactly 9+bControlSize bytes (bmVideoStandards)
    - INPUT/OUTPUT_HEADER with bControlSize 0 and exactly 13/9 bytes
      (bmaControls)
    """
    out, pos = [], 0
    while pos + 2 <= len(blob) and blob[pos] >= 2:
        d = blob[pos:pos + blob[pos]]
        pos += blob[pos]
        if len(d) > 2 and d[1] == 0x24:
            st = d[2]
            if st == 5 and len(d) > 7 and d[0] == 9 + d[7]:
                d = d + [byte()]
            elif st == 1 and len(d) == 13 and d[3] and d[12] == 0:
                d = d + [byte()]
            elif st == 2 and len(d) == 9 and d[3] and d[8] == 0:
                d = d + [byte()]
            d[0] = len(d)
        out += d
    return out + blob[pos:]


def ep_extra(cls, sub):
    out = []
    for _ in range(rng.choice([0, 0, 1, 2])):
        r = rng.random()
        if r < 0.35:
            out += desc(0x25, rng.choice([0, 1, 2, 3]), rng.choice([None, 5, 7, 6]))
        elif r < 0.55:
            out += desc(0x30, None, rng.choice([6, 6, 4]))
        elif r < 0.7:
            out += desc(0x24, rng.choice([0, 1, 2, 6, 0x0f]), rng.choice([None, 4, 5]))
        else:
            out += desc(rng.choice([0x21, 0x0b, 0x09, 0x41]), None, rng.choice([None, 3, 8, 54]))
    return maybe_corrupt(out)


def config():
    speed = rng.choice([0x0110, 0x0200, 0x0201, 0x0210, 0x0300, 0x0310, 0x0320])
    extra = []
    for _ in range(rng.choice([0, 0, 1, 2])):
        dtype = rng.choice([0x0b, 0x0c, 0x0e, 0x09, 0x24, 0x44])
        extra += desc(dtype, None, {0x0b: 8, 0x0c: 5, 0x0e: 5, 0x09: 3}.get(dtype) if rng.random() < 0.7 else None)
    lines = ["CONFIG %x %x %x %x %s" % (speed, rng.choice([0x80, 0xc0, 0xa0, 0x40, 0xf0]),
                                         rng.randrange(256), rng.choice([0, 0, 4]), hexs(maybe_corrupt(extra)))]
    for i in range(rng.choice([1, 1, 2, 3])):
        cls, subs, protos = rng.choice(CLASSES)
        sub, proto = rng.choice(subs), rng.choice(protos)
        lines.append("IFACE %x %x %x %x %x %s" % (cls, sub, proto, i, rng.choice([0, 0, 1]),
                                                  hexs(iface_extra(cls, sub, proto))))
        for _ in range(rng.choice([0, 1, 2, 3])):
            lines.append("EP %x %x %x %x %x %s" % (
                rng.choice([7, 7, 9]), rng.choice([0x01, 0x81, 0x82, 0x03, 0x8f]),
                rng.randrange(0, 64), rng.choice([0x40, 0x200, 0x400, 0x1400, 0x13ff, rng.randrange(65536)]),
                rng.randrange(0, 16), hexs(ep_extra(cls, sub))))
    lines.append("END")
    return lines


def hub():
    tt = rng.choice([0, 1, 2, 3])
    b = [rng.choice([9, 12, 13]), rng.choice([0x29, 0x2a])] + [rng.randrange(256) for _ in range(11)]
    b[2] = rng.choice([1, 4, 7, 8, 15, 23, rng.randrange(256)])
    return ["HUB %x %s" % (tt, hexs(b))]


REPORT_TAGS = [0x04, 0x08, 0x14, 0x24, 0x18, 0x28, 0x74, 0x94, 0x80, 0x90, 0xb0, 0xa0, 0xc0,
               0x54, 0x64, 0x84, 0x34, 0x44, 0xf0, 0xfc]


def report():
    out = []
    for _ in range(rng.randrange(0, 40)):
        tag = rng.choice(REPORT_TAGS) if rng.random() < 0.9 else rng.randrange(256) & ~3
        size = rng.choice([0, 1, 1, 2, 3])
        out.append(tag | size)
        n = 4 if size == 3 else size
        if tag == 0x04 and n:
            vals = [rng.choice([1, 2, 7, 8, 9, 0x0c, 0x0d, 0x20, 0xff, rng.randrange(256)])]
        else:
            vals = []
        out += (vals + [byte() for _ in range(n)])[:n]
    if rng.random() < 0.1 and out:
        out = out[:-1]  # truncated item
    return ["REPORT %s" % hexs(out)]


def bos():
    out = []
    for _ in range(rng.randrange(0, 5)):
        t = rng.choice([1, 2, 3, 4, 5, 0x0a, 0x0d, 0x0f, 0x10, 0x11, 0x06])
        if t == 0x0a:
            n = rng.choice([0, 1, 3])
            d = [12 + 4 * (n + 1), 0x10, t] + body(9 + 4 * (n + 1))
            d[4] = (d[4] & 0xe0) | n
        elif t == 0x0d:
            n = rng.choice([0, 1, 2, 3, 0x35])
            d = [44 + 4 * n if n < 0x35 else 48, 0x10, t] + body(41 + 4 * min(n, 1) + 4 * n)
            d = d[:max(d[0], 48)]
            d[0] = max(d[0], 48)
            d += body(d[0] - len(d))
            d[4] = n
        elif t == 0x10:
            nc = rng.choice([0, 1, 2])
            d = [10 + nc, 0x10, t, 0x00, 0x01] + body(5 + nc)
            d[9] = nc
        else:
            ln = {1: 11, 2: 7, 3: 10, 4: 20, 5: rng.choice([20, 24, 28]), 0x0f: 8, 0x11: 8}.get(t, 5)
            if rng.random() < 0.15:
                ln = rng.randrange(3, 30)
            d = [ln, 0x10, t] + body(ln - 3)
        out += d
    return ["BOS %x %s" % (rng.choice([0, 1]), hexs(maybe_corrupt(out)))]


for _ in range(COUNT):
    kind = rng.random()
    if kind < 0.6:
        lines = config()
    elif kind < 0.7:
        lines = hub()
    elif kind < 0.85:
        lines = report()
    else:
        lines = bos()
    print("\n".join(lines))
