#!/usr/bin/env python3
"""Stage Debian maintainer scripts with the packaged config embedded in preinst."""

import argparse
from pathlib import Path
import shutil


def generate(source: Path, destination: Path) -> None:
    stub = (source / "vector.yaml").read_bytes()
    preinst = (source / "scripts/preinst").read_bytes()
    marker = b"@VECTOR_CONFIG_STUB@\n"
    if preinst.count(marker) != 1:
        raise ValueError("preinst must contain exactly one config-stub placeholder")
    if not stub.endswith(b"\n") or b"VECTOR_CONFIG_STUB" in stub.splitlines():
        raise ValueError("config stub must end with a newline and not close its heredoc")

    destination.mkdir(parents=True, exist_ok=True)
    for script in (source / "scripts").iterdir():
        shutil.copy2(script, destination / script.name)
    (destination / "preinst").write_bytes(preinst.replace(marker, stub))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("destination", type=Path)
    args = parser.parse_args()
    generate(Path(__file__).resolve().parent.parent / "distribution/debian", args.destination)
