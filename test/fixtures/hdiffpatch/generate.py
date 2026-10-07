"""Regenerate the small cross-language fixtures with vendor/hdiffz.exe v4.12.0."""
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent
helper = root.parents[2] / "vendor/hdiffz.exe"
old = bytes(range(256)) * 32
(root / "old.bin").write_bytes(old)
(root / "new.bin").write_bytes(b"inserted\0header" + old[4096:] + old[:4096] + b"updated trailer")
(root / "empty.bin").write_bytes(b"")
for old_name, new_name, patch, mode in [
    ("old.bin", "new.bin", "speed.hdiff", "-s-64"),
    ("old.bin", "new.bin", "size.hdiff", "-m-4"),
    ("old.bin", "empty.bin", "to-empty.hdiff", "-s-64"),
    ("empty.bin", "new.bin", "from-empty.hdiff", "-s-64"),
]:
    subprocess.run([str(helper), "-f", "-p-1", mode, str(root / old_name), str(root / new_name), str(root / patch)], check=True)
