#!/usr/bin/env python3
# SPDX-License-Identifier: MIT OR Apache-2.0
"""Generate the homelab sensor panels for asterctl: homelab.json plus background and bar images.

Standard library only. Edit the layout below and run `python3 generate.py` in this directory.

Design notes:
- Healthy states are quiet grey, not green. Nine green rows shout as loudly as the one red one.
- The summary panel shows a single number: how many things are wrong. Healthy is a big green 0.
- State values come in `_ok` / `_warn` / `_bad` variants from aster-homelab. All three variants
  are placed at the same position with different colors; only the current one has a value.
"""

import json
import struct
import zlib
from pathlib import Path

W, H = 960, 376
HERE = Path(__file__).parent

# colors
BG = (10, 14, 19)
TILE = (19, 26, 34)
STRIPE = (15, 21, 28)
EDGE = (33, 44, 57)
TRACK = (38, 50, 64)
BAR = (79, 156, 249)

CAPTION = "#7b8794"
VALUE = "#e6ebf0"
DIM = "#56616d"
OK = "#9aa5b1"
WARN = "#f2a93b"
BAD = "#ef5350"
GOOD = "#3ddc84"

VALUE_FONT = "HarmonyOS_Sans_SC_Bold"
CAPTION_FONT = "DejaVuSans"


# --- minimal PNG canvas ---------------------------------------------------------------------

class Canvas:
    def __init__(self, w, h, color):
        self.w, self.h = w, h
        self.rows = [bytearray(bytes(color) * w) for _ in range(h)]

    def rect(self, x, y, w, h, color):
        x0, x1 = max(0, x), min(self.w, x + w)
        for yy in range(max(0, y), min(self.h, y + h)):
            self.rows[yy][x0 * 3:x1 * 3] = bytes(color) * (x1 - x0)

    def tile(self, x, y, w, h, fill=TILE, edge=EDGE):
        self.rect(x, y, w, h, edge)
        self.rect(x + 1, y + 1, w - 2, h - 2, fill)

    def save(self, path):
        raw = b"".join(b"\x00" + bytes(row) for row in self.rows)

        def chunk(kind, data):
            return (struct.pack(">I", len(data)) + kind + data
                    + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF))

        png = (b"\x89PNG\r\n\x1a\n"
               + chunk(b"IHDR", struct.pack(">IIBBBBB", self.w, self.h, 8, 2, 0, 0, 0))
               + chunk(b"IDAT", zlib.compress(raw, 9))
               + chunk(b"IEND", b""))
        Path(path).write_bytes(png)


# --- panel elements -------------------------------------------------------------------------

class Panel:
    def __init__(self, name, img, requires=None):
        self.data = {"name": name, "img": f"img/{img}", "sensor": []}
        if requires:
            self.data["requires"] = requires
        self.canvas = Canvas(W, H, BG)
        self.img = img
        self.captions = 0

    def text(self, label, x, y, w, h, size, color=VALUE, align="left", font=VALUE_FONT,
             decimals=None, unit="", value=None):
        sensor = {
            "mode": 1, "label": label, "x": x, "y": y, "width": w, "height": h,
            "fontFamily": font, "fontSize": size, "fontColor": color, "textAlign": align,
            "integerDigits": -1, "decimalDigits": decimals if decimals is not None else 0,
            "unit": unit,
        }
        if value is not None:
            sensor["value"] = value
        if decimals is None:
            # keep non-numeric values as they are, numbers without decimals
            sensor["decimalDigits"] = 0
        self.data["sensor"].append(sensor)

    def caption(self, text, x, y, w=200, h=20, size=14, color=CAPTION, align="left"):
        self.captions += 1
        self.text(f"TEXT_{self.captions}", x, y, w, h, size, color, align, CAPTION_FONT, value=text)

    def state(self, key, x, y, w, h, size, align="left", ok=OK, warn=WARN, bad=BAD,
              font=VALUE_FONT, decimals=None, unit=""):
        for suffix, color in (("ok", ok), ("warn", warn), ("bad", bad)):
            self.text(f"{key}_{suffix}", x, y, w, h, size, color, align, font, decimals, unit)

    def bar(self, label, x, y, w, h, pic):
        self.canvas.rect(x, y, w, h, TRACK)
        self.data["sensor"].append({
            "mode": 3, "label": label, "x": x, "y": y, "direction": 1,
            "minValue": 0, "maxValue": 100, "pic": f"img/{pic}",
        })

    def header(self, title):
        self.canvas.rect(0, 46, W, 1, EDGE)
        self.caption(title, 24, 8, 300, 32, 20, CAPTION)
        self.text("DATE_y_m_d_2", 560, 8, 200, 32, 18, DIM, "right", CAPTION_FONT)
        self.text("DATE_h_m_3", 776, 8, 160, 32, 22, VALUE, "right")

    def save(self):
        self.canvas.save(HERE / "img" / self.img)
        return self.data


def bar_image(name, w, h, color=BAR):
    c = Canvas(w, h, color)
    c.save(HERE / "img" / name)
    return name


# number of rows on the nodes panel: one per Proxmox host or cluster node
NODES = 3


# --- panels ---------------------------------------------------------------------------------

def summary_panel():
    p = Panel("Summary", "summary.png")
    p.header("HOMELAB")

    # the big number: how many things are wrong
    p.canvas.tile(16, 58, 300, 302)
    p.state("homelab_problems", 16, 70, 300, 230, 210, "center", ok=GOOD)
    p.caption("THINGS WRONG", 16, 318, 300, 26, 15, CAPTION, "center")

    # what is wrong
    for i in range(3):
        p.state(f"homelab_problem[{i}]", 336, 60 + i * 34, 600, 32, 20)
    p.text("homelab_problems_more", 736, 128, 200, 32, 16, DIM, "right", CAPTION_FONT)

    tiles = [
        ("GUESTS UP", "guests_summary", None, ""),
        ("POOLS ONLINE", "pools_summary", None, ""),
        ("NAS ALERTS", None, "truenas_alerts", ""),
        ("HOTTEST DISK", "disks_max_temp", None, "°C"),
        ("NAS UPTIME", "truenas_uptime", None, ""),
        ("BACKUPS FRESH", None, "backups_summary", ""),
    ]
    for idx, (title, label, state, unit) in enumerate(tiles):
        x = 332 + (idx % 3) * 208
        y = 176 + (idx // 3) * 94
        p.canvas.tile(x, y, 196, 84)
        p.caption(title, x + 14, y + 10, 170, 18, 13)
        if state:
            p.state(state, x + 14, y + 32, 170, 44, 34, decimals=0)
        else:
            p.text(label, x + 14, y + 32, 170, 44, 34, VALUE, unit=unit)
    return p.save()


def host_panel():
    p = Panel("Host", "host.png")
    p.header("HOST")
    p.text("system_hostname", 96, 8, 300, 32, 20, VALUE)

    bar = bar_image("bar_host.png", 194, 8)

    def tile(x, y, title):
        p.canvas.tile(x, y, 222, 146)
        p.caption(title, x + 14, y + 10, 194, 18, 13)

    # row 1: CPU, memory, CPU temperature, root disk
    row1 = [
        ("CPU", "host_cpu_percent", "%", True),
        ("MEMORY", "host_mem_percent", "%", True),
        ("CPU TEMP", "host_cpu_temp", "", False),
        ("ROOT DISK", "host_root_percent", "%", True),
    ]
    for idx, (title, label, unit, with_bar) in enumerate(row1):
        x = 16 + idx * 234
        tile(x, 56, title)
        p.text(label, x + 14, 80, 194, 64, 56, VALUE, unit=unit)
        if with_bar:
            p.bar(label, x + 14, 176, 194, 8, bar)
    p.text("mem_used", 248 + 14, 148, 90, 20, 14, DIM, font=CAPTION_FONT)
    p.text("mem_total", 248 + 108, 148, 100, 20, 14, DIM, "right", CAPTION_FONT)

    # row 2: load, uptime, network, guests
    tile(16, 214, "LOAD 1 / 5 / 15 MIN")
    for i, label in enumerate(("load_avg_one", "load_avg_five", "load_avg_fifteen")):
        p.text(label, 30 + i * 66, 250, 62, 44, 26 if i == 0 else 22, VALUE if i == 0 else OK,
               decimals=1)

    tile(250, 214, "UPTIME")
    p.text("system_uptime_short", 264, 244, 194, 56, 40, VALUE)

    tile(484, 214, "NETWORK")
    p.caption("DOWN", 498, 250, 60, 22, 13, DIM)
    p.text("host_net_down", 556, 248, 136, 26, 20, VALUE, "right")
    p.caption("UP", 498, 286, 60, 22, 13, DIM)
    p.text("host_net_up", 556, 284, 136, 26, 20, VALUE, "right")

    tile(718, 214, "GUESTS RUNNING")
    p.text("guests_summary", 732, 244, 194, 56, 40, VALUE)
    p.caption("VM", 732, 316, 30, 20, 13, DIM)
    p.text("vms_running", 760, 314, 40, 22, 16, OK)
    p.caption("CT", 812, 316, 30, 20, 13, DIM)
    p.text("cts_running", 840, 314, 40, 22, 16, OK)
    return p.save()


def guests_panel(page):
    first = page * 14
    name = "Guests" if page == 0 else f"Guests {page + 1}"
    p = Panel(name, f"guests{page + 1}.png", "pve_reachable" if page == 0 else f"guest[{first}]_name")
    p.header("GUESTS" if page == 0 else f"GUESTS {page + 1}")
    p.state("pve_status", 130, 8, 120, 32, 16, ok=DIM)
    p.text("guests_summary", 430, 8, 110, 32, 22, VALUE, "right")

    for col in range(2):
        x0 = 16 + col * 472
        p.caption("ID", x0 + 8, 52, 50, 18, 11, DIM)
        p.caption("NAME", x0 + 60, 52, 120, 18, 11, DIM)
        p.caption("STATUS", x0 + 214, 52, 80, 18, 11, DIM)
        p.caption("CPU", x0 + 292, 52, 48, 18, 11, DIM, "right")
        p.caption("MEM", x0 + 344, 52, 48, 18, 11, DIM, "right")
        p.caption("BACKUP", x0 + 392, 52, 56, 18, 11, DIM, "right")
        for row in range(7):
            i = first + col * 7 + row
            y = 74 + row * 42
            p.canvas.rect(x0, y, 456, 40, TILE if row % 2 == 0 else STRIPE)
            g = f"guest[{i}]"
            p.text(f"{g}_id", x0 + 8, y + 3, 50, 18, 14, OK)
            p.text(f"{g}_tag", x0 + 8, y + 21, 56, 16, 11, DIM, font=CAPTION_FONT)
            p.text(f"{g}_name", x0 + 60, y + 2, 152, 36, 18, VALUE)
            # stopped guests are informational (dim), unless listed in `expect_running`
            p.state(f"{g}_status", x0 + 214, y + 2, 80, 36, 13, ok=OK, warn=DIM, bad=BAD)
            p.text(f"{g}_cpu_percent", x0 + 292, y + 2, 48, 36, 16, VALUE, "right", unit="%")
            p.text(f"{g}_mem_percent", x0 + 344, y + 2, 48, 36, 16, VALUE, "right", unit="%")
            # backup age: grey when fresh, amber when too old; dim NONE without any backup
            p.state(f"{g}_backup", x0 + 392, y + 2, 56, 36, 14, "right", ok=OK)
            p.text(f"{g}_backup_none", x0 + 392, y + 2, 56, 36, 12, DIM, "right", CAPTION_FONT)
    return p.save()


def nodes_panel():
    # only shown with multiple Proxmox hosts or cluster nodes
    p = Panel("Nodes", "nodes.png", "pve_node[1]_name")
    p.header("PROXMOX NODES")
    p.state("pve_status", 250, 8, 160, 32, 16, ok=DIM)

    bar = bar_image("bar_node.png", 180, 8)
    for i in range(NODES):
        x, y = 16, 56 + i * 104
        k = f"pve_node[{i}]"
        p.canvas.tile(x, y, 928, 96)
        p.text(f"{k}_name", x + 16, y + 12, 180, 40, 30, VALUE)
        p.state(f"{k}_status", x + 16, y + 56, 180, 24, 14, font=CAPTION_FONT)
        for j, (title, label) in enumerate((("CPU", "cpu_percent"), ("MEMORY", "mem_percent"))):
            cx = x + 210 + j * 210
            p.caption(title, cx, y + 12, 180, 18, 12, DIM)
            p.text(f"{k}_{label}", cx, y + 32, 180, 36, 28, VALUE, unit="%")
            p.bar(f"{k}_{label}", cx, y + 76, 180, 8, bar)
        p.caption("GUESTS", x + 640, y + 12, 120, 18, 12, DIM)
        p.text(f"{k}_guests", x + 640, y + 32, 120, 40, 30, VALUE)
        p.caption("UPTIME", x + 762, y + 12, 150, 18, 12, DIM, "right")
        p.text(f"{k}_uptime", x + 762, y + 32, 150, 40, 26, VALUE, "right")
    return p.save()


def protection_panel():
    p = Panel("Data protection", "protection.png")
    p.header("DATA PROTECTION")

    # left: pool scrubs and Proxmox backups
    p.caption("POOL SCRUBS", 30, 54, 200, 18, 12)
    for i in range(3):
        x, y = 16, 74 + i * 44
        k = f"pool[{i}]"
        p.canvas.tile(x, y, 460, 38)
        p.text(f"{k}_name", x + 14, y + 2, 200, 34, 18, VALUE)
        p.text(f"{k}_scrub_eta", x + 200, y + 2, 110, 34, 13, DIM, "right", CAPTION_FONT)
        p.state(f"{k}_scrub", x + 316, y + 2, 130, 34, 15, "right")

    p.caption("PROXMOX BACKUP JOBS", 30, 210, 220, 18, 12)
    p.state("backups_summary", 300, 206, 92, 26, 18, "right", ok=VALUE)
    p.caption("FRESH", 398, 210, 64, 18, 12, DIM)
    for i in range(3):
        x, y = 16, 234 + i * 42
        k = f"pve_backup_job[{i}]"
        p.canvas.tile(x, y, 460, 36)
        p.text(f"{k}_node", x + 14, y + 2, 120, 32, 17, VALUE)
        p.state(f"{k}_state", x + 160, y + 2, 140, 32, 15)
        p.text(f"{k}_age", x + 346, y + 2, 100, 32, 15, OK, "right")

    # right: TrueNAS replication, cloud sync, rsync and snapshot tasks
    p.caption("TRUENAS TASKS", 502, 54, 200, 18, 12)
    p.state("tasks_summary", 840, 50, 60, 26, 18, "right", ok=VALUE)
    p.caption("OK", 906, 54, 40, 18, 12, DIM)
    for i in range(7):
        x, y = 488, 74 + i * 41
        k = f"task[{i}]"
        p.canvas.rect(x, y, 456, 38, TILE if i % 2 == 0 else STRIPE)
        p.text(f"{k}_kind", x + 10, y + 2, 54, 34, 11, DIM, font=CAPTION_FONT)
        p.text(f"{k}_name", x + 64, y + 2, 228, 34, 15, VALUE)
        p.state(f"{k}_state", x + 296, y + 2, 92, 34, 13)
        p.text(f"{k}_age", x + 388, y + 2, 58, 34, 14, OK, "right")
    return p.save()


def pbs_panel():
    p = Panel("Backup server", "pbs.png", "pbs_reachable")
    p.header("BACKUP SERVER")
    p.state("pbs_status", 220, 8, 110, 32, 16, ok=DIM)
    p.text("pbs_version", 330, 8, 120, 32, 14, DIM, font=CAPTION_FONT)

    # left: datastores
    p.caption("DATASTORES", 30, 54, 200, 18, 12)
    bar = bar_image("bar_pool.png", 330, 10)
    for i in range(2):
        x, y = 16, 74 + i * 104
        k = f"datastore[{i}]"
        p.canvas.tile(x, y, 460, 96)
        p.text(f"{k}_name", x + 14, y + 6, 270, 30, 22, VALUE)
        p.state(f"{k}_status", x + 290, y + 6, 156, 30, 14, "right", ok=DIM)
        p.bar(f"{k}_used_percent", x + 14, y + 46, 330, 10, bar)
        p.state(f"{k}_used_level", x + 356, y + 36, 90, 28, 18, "right", ok=VALUE)
        # usage line, or the error of an unavailable datastore
        p.text(f"{k}_usage", x + 14, y + 66, 280, 24, 14, OK)
        p.text(f"{k}_full_in", x + 290, y + 66, 156, 24, 13, DIM, "right", CAPTION_FONT)
        p.text(f"{k}_error", x + 14, y + 66, 432, 24, 13, BAD, font=CAPTION_FONT)

    p.canvas.tile(16, 290, 460, 70)
    p.caption("JOBS OK", 30, 300, 120, 18, 12)
    p.state("pbs_jobs_summary", 30, 320, 160, 36, 28, ok=VALUE)
    p.caption("DATASTORES AVAILABLE", 250, 300, 212, 18, 12, CAPTION, "right")
    p.state("datastores_summary", 250, 320, 212, 36, 28, "right", ok=VALUE)

    # right: verify, sync, prune and garbage collection jobs
    p.caption("JOBS", 502, 54, 200, 18, 12)
    for i in range(7):
        x, y = 488, 74 + i * 41
        k = f"pbs_job[{i}]"
        p.canvas.rect(x, y, 456, 38, TILE if i % 2 == 0 else STRIPE)
        p.text(f"{k}_kind", x + 10, y + 2, 56, 34, 11, DIM, font=CAPTION_FONT)
        p.text(f"{k}_name", x + 66, y + 2, 200, 34, 15, VALUE)
        p.state(f"{k}_state", x + 270, y + 2, 112, 34, 13)
        p.text(f"{k}_age", x + 388, y + 2, 58, 34, 14, OK, "right")
    return p.save()


def truenas_panel():
    p = Panel("TrueNAS", "truenas.png", "truenas_reachable")
    p.header("TRUENAS")
    p.text("truenas_hostname", 140, 8, 160, 32, 20, VALUE)
    p.text("truenas_version", 300, 8, 120, 32, 14, DIM, font=CAPTION_FONT)
    p.state("truenas_status", 430, 8, 110, 32, 16, "right", ok=DIM)

    bar = bar_image("bar_pool.png", 330, 10)
    for i in range(3):
        x, y = 16, 56 + i * 80
        k = f"pool[{i}]"
        p.canvas.tile(x, y, 460, 72)
        p.text(f"{k}_name", x + 14, y + 6, 280, 30, 22, VALUE)
        p.state(f"{k}_status", x + 290, y + 6, 156, 30, 15, "right")
        p.bar(f"{k}_used_percent", x + 14, y + 48, 330, 10, bar)
        p.state(f"{k}_used_level", x + 356, y + 38, 90, 28, 18, "right", ok=VALUE)

    p.canvas.tile(16, 296, 460, 64)
    for i in range(2):
        p.state(f"truenas_alert[{i}]", 30, 302 + i * 28, 436, 26, 14, font=CAPTION_FONT)

    p.caption("DISK TEMPERATURES", 488, 52, 220, 24, 13)
    p.caption("MAX", 820, 52, 50, 24, 13, DIM, "right")
    p.text("disks_max_temp", 872, 50, 64, 28, 18, VALUE, "right", unit="°")
    for i in range(12):
        x = 488 + (i % 4) * 116
        y = 82 + (i // 4) * 74
        p.canvas.tile(x, y, 108, 66)
        p.text(f"disk[{i}]_name", x + 10, y + 6, 90, 18, 13, DIM, font=CAPTION_FONT)
        p.state(f"disk[{i}]_temp_level", x + 10, y + 26, 90, 34, 26, ok=VALUE)

    p.canvas.tile(488, 312, 456, 48)
    p.caption("APPS", 502, 324, 60, 24, 13)
    p.text("apps_summary", 560, 320, 100, 30, 20, VALUE)
    p.caption("UPTIME", 690, 324, 80, 24, 13)
    p.text("truenas_uptime", 770, 320, 160, 30, 20, VALUE, "right")
    return p.save()


def main():
    panels = [summary_panel(), host_panel(), nodes_panel(), guests_panel(0), guests_panel(1),
              truenas_panel(), protection_panel(), pbs_panel()]
    config = {
        "setup": {"switchTime": "10", "refresh": 1},
        "mianban": list(range(1, len(panels) + 1)),
        "diy": panels,
    }
    (HERE / "homelab.json").write_text(json.dumps(config, indent=1, ensure_ascii=False) + "\n")
    print(f"Wrote homelab.json with {len(panels)} panels")


if __name__ == "__main__":
    main()
