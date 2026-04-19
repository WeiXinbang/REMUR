#!/usr/bin/env python3
"""
Convert Spike log output to REMUR M7 difftest event format.

Input examples (Spike -l):
  core   0: 0x0000000080000000 (0x0500006f) ...
  core   0: 3 0x0000000080000000 (0x0500006f) ...
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path


PATTERN_WITH_PRIV = re.compile(
    r"core\s+\d+:\s+([0-3])\s+0x([0-9a-fA-F]+)\s+\(0x([0-9a-fA-F]+)\)"
)
PATTERN_NO_PRIV = re.compile(r"core\s+\d+:\s+0x([0-9a-fA-F]+)\s+\(0x([0-9a-fA-F]+)\)")
PRIV_MAP = {"0": "U", "1": "S", "3": "M"}


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Convert Spike log to REMUR trace format")
    parser.add_argument("--input", required=True, help="Spike log input file")
    parser.add_argument("--output", required=True, help="REMUR trace output file")
    parser.add_argument(
        "--limit", type=int, default=0, help="Maximum emitted instructions (0 = unlimited)"
    )
    parser.add_argument(
        "--default-priv",
        choices=["U", "S", "M"],
        default="M",
        help="Privilege used when log line does not include privilege column",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    src = Path(args.input)
    out = Path(args.output)

    records: list[tuple[int, int, str]] = []
    for line in src.read_text(encoding="utf-8", errors="ignore").splitlines():
        m = PATTERN_WITH_PRIV.search(line)
        if m:
            priv_raw, pc_raw, inst_raw = m.groups()
            priv = PRIV_MAP.get(priv_raw, args.default_priv)
            records.append((int(pc_raw, 16) & 0xFFFF_FFFF, int(inst_raw, 16) & 0xFFFF_FFFF, priv))
            continue

        m = PATTERN_NO_PRIV.search(line)
        if m:
            pc_raw, inst_raw = m.groups()
            records.append(
                (int(pc_raw, 16) & 0xFFFF_FFFF, int(inst_raw, 16) & 0xFFFF_FFFF, args.default_priv)
            )

    if args.limit and args.limit > 0:
        records = records[: args.limit]

    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open("w", encoding="utf-8", newline="\n") as f:
        for idx, (pc, inst, priv) in enumerate(records):
            if idx + 1 < len(records):
                next_pc = records[idx + 1][0]
            else:
                next_pc = (pc + 4) & 0xFFFF_FFFF
            f.write(f"I,{idx},0x{pc:08x},0x{inst:08x},0x{next_pc:08x},{priv},{priv},-,-\n")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
