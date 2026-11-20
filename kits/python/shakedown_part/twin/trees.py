"""Copying a file tree for a save: files that never change in place are hard-linked, the rest copied."""
import os
import shutil
from dataclasses import dataclass
from pathlib import Path
from typing import Callable


@dataclass(frozen=True)
class LinkCopy:
    linked: int
    copied: int
    bytes: int


def _place(s: Path, d: Path, link: bool) -> int:
    """Link or copy one file (mode kept); the bytes written."""
    if d.exists() or d.is_symlink():
        d.unlink()
    if s.is_symlink():
        os.symlink(os.readlink(s), d)
        return 0
    if link:
        os.link(s, d)
        return 0
    shutil.copy2(s, d)
    return s.stat().st_size


def link_copy(src: Path, dst: Path, immutable: Callable[[Path], bool]) -> LinkCopy:
    """Copy `src` into `dst`. `immutable(path inside the tree)` true: hard-linked to the source, so
    saves share the file; else copied. Folders (empty ones too) and modes are kept."""
    src, dst = Path(src), Path(dst)
    linked = copied = written = 0
    for root, dirs, files in os.walk(src):
        rel = Path(root).relative_to(src)
        (dst / rel).mkdir(parents=True, exist_ok=True)
        for name in files:
            is_link = immutable(rel / name)
            written += _place(Path(root) / name, dst / rel / name, is_link)
            linked, copied = linked + is_link, copied + (not is_link)
    return LinkCopy(linked, copied, written)
