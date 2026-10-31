#!/usr/bin/env python3
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CARGO = ROOT / "Cargo.toml"
VERSION_RE = re.compile(r'^(version\s*=\s*")(\d+)\.(\d+)\.(\d+)(")', re.M)


def base_version() -> tuple[int, int, int]:
    m = VERSION_RE.search(CARGO.read_text(encoding="utf-8"))
    if not m:
        raise SystemExit("version not found in Cargo.toml")
    return int(m.group(2)), int(m.group(3)), int(m.group(4))


def remote_tags() -> list[str]:
    out = subprocess.run(["git", "ls-remote", "--tags", "origin"], capture_output=True, text=True, check=True).stdout
    return [line.split("refs/tags/")[1].removesuffix("^{}") for line in out.splitlines() if "refs/tags/" in line]


def next_version(base: tuple[int, int, int], tags: list[str]) -> str:
    x, y, z = base
    if f"v{x}.{y}.{z}" not in tags:
        return f"{x}.{y}.{z}"
    patches = [int(m.group(1)) for t in tags if (m := re.fullmatch(rf"v{x}\.{y}\.(\d+)", t))]
    return f"{x}.{y}.{max(patches) + 1}"


def self_test() -> None:
    tags = ["v0.1.0", "v0.2.0", "v0.2.1", "v0.2.2", "v0.2.3", "v0.2.3^{}"]
    assert next_version((0, 2, 0), tags) == "0.2.4"
    assert next_version((0, 3, 0), tags) == "0.3.0"
    assert next_version((0, 3, 0), tags + ["v0.3.0"]) == "0.3.1"
    assert next_version((1, 0, 0), []) == "1.0.0"
    print("ok")


def main() -> None:
    if "--self-test" in sys.argv:
        self_test()
        return
    version = next_version(base_version(), remote_tags())
    if "--apply" in sys.argv:
        text = CARGO.read_text(encoding="utf-8")
        CARGO.write_text(VERSION_RE.sub(lambda m: f"{m.group(1)}{version}{m.group(5)}", text, count=1), encoding="utf-8")
    print(f"version={version}")


if __name__ == "__main__":
    main()
