import os
import stat

from arena_part.twin.trees import link_copy


def make(root, rel, text, mode=0o644):
    p = root / rel
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(text)
    p.chmod(mode)
    return p


def test_immutable_files_are_hard_linked_between_saves(tmp_path):
    src, dst = tmp_path / "src", tmp_path / "dst"
    make(src, "objects/ab/cd", "blob", 0o444)
    make(src, "refs/heads/main", "abc\n")
    c = link_copy(src, dst, immutable=lambda p: "objects" in p.parts)
    assert os.stat(src / "objects/ab/cd").st_ino == os.stat(dst / "objects/ab/cd").st_ino
    assert os.stat(src / "refs/heads/main").st_ino != os.stat(dst / "refs/heads/main").st_ino
    assert (c.linked, c.copied, c.bytes) == (1, 1, 4)


def test_modes_and_empty_folders_are_kept(tmp_path):
    src, dst = tmp_path / "src", tmp_path / "dst"
    make(src, "hooks/post", "#!/bin/sh\n", 0o755)
    (src / "refs/tags").mkdir(parents=True)
    link_copy(src, dst, immutable=lambda p: False)
    assert stat.S_IMODE(os.stat(dst / "hooks/post").st_mode) == 0o755
    assert (dst / "refs/tags").is_dir()


def test_immutable_gets_the_path_inside_the_tree(tmp_path):
    seen = []
    make(tmp_path / "src", "a/b", "x")
    link_copy(tmp_path / "src", tmp_path / "dst", immutable=lambda p: seen.append(p) or False)
    assert [str(p) for p in seen] == ["a/b"]
