"""A template can't reach outside its folder, stall the builder or break the listing of the others: links,
overlapping folders, cycles, bombs and sizes are template errors."""
import os
import time
from dataclasses import replace

import pytest
from support import copy_with, ctx_any

from worldbench import limits
from worldbench.expand import expand
from worldbench.generate import generate
from worldbench.install import write_package
from worldbench.template import TemplateError, list_templates, load_template


def test_a_folder_include_with_a_link_in_it_is_refused(tmp_path):
    (tmp_path / "secret.txt").write_text("outside", encoding="utf-8")
    base = tmp_path / "t"
    (base / "repos" / "a").mkdir(parents=True)
    os.symlink(tmp_path / "secret.txt", base / "repos" / "a" / "key")
    with pytest.raises(TemplateError, match="repos/a/key is a link"):
        expand("@repos/", ctx_any(), base, [])


def test_installing_a_folder_with_a_link_in_it_fails_and_leaves_nothing(shop, tmp_path):
    (tmp_path / "secret.txt").write_text("outside", encoding="utf-8")
    src = tmp_path / "seed"
    src.mkdir()
    os.symlink(tmp_path / "secret.txt", src / "key")
    r = generate(shop, {})
    with pytest.raises(TemplateError, match="is a link"):
        write_package(replace(r, files=[(src, "files/seed/")]), shop, tmp_path / "w")
    assert not (tmp_path / "w").exists()


def test_a_folder_include_is_copied_to_its_resolved_place(tmp_path):
    (tmp_path / "a" / "b").mkdir(parents=True)
    (tmp_path / "x").mkdir()
    os.symlink(tmp_path / "a" / "b", tmp_path / "l")
    files = []
    assert expand("@x/../x/", ctx_any(), tmp_path, files) == "${WORLD_DIR}/files/x/"
    assert expand("@l/../../x/", ctx_any(), tmp_path, files) == "${WORLD_DIR}/files/x/"
    assert files == [(tmp_path / "x", "files/x/")]


def test_overlapping_folder_includes_are_a_template_error(tmp_path):
    (tmp_path / "data" / "sub").mkdir(parents=True)
    with pytest.raises(TemplateError, match="@data/sub/ overlaps @data/") as e:
        expand({"a": "@data/", "b": "@data/sub/"}, ctx_any(), tmp_path, [])
    assert e.value.path == "b"
    with pytest.raises(TemplateError, match="@data/ overlaps @data/sub/"):
        expand(["@data/sub/", "@data/"], ctx_any(), tmp_path, [])


def test_an_include_that_includes_itself_is_a_template_error(tmp_path):
    (tmp_path / "a.yaml").write_text("k: '@b.yaml'\n", encoding="utf-8")
    (tmp_path / "b.yaml").write_text("- '@a.yaml'\n", encoding="utf-8")
    with pytest.raises(TemplateError, match="@a.yaml includes itself") as e:
        expand({"x": "@a.yaml"}, ctx_any(), tmp_path, [])
    assert e.value.path == "x@a.yaml.k@b.yaml[0]"


def test_an_expansion_too_large_is_a_template_error(tmp_path, monkeypatch):
    monkeypatch.setattr(limits, "MAX_EXPANDED", 50)
    (tmp_path / "wide.yaml").write_text("[" + ", ".join(["'@one.yaml'"] * 30) + "]\n", encoding="utf-8")
    (tmp_path / "one.yaml").write_text("[1, 2]\n", encoding="utf-8")
    with pytest.raises(TemplateError, match="more than 50 values"):
        expand("@wide.yaml", ctx_any(), tmp_path, [])


def test_files_and_folders_too_large_are_template_errors(tmp_path, monkeypatch):
    monkeypatch.setattr(limits, "MAX_FILE_BYTES", 100)
    monkeypatch.setattr(limits, "MAX_FOLDER_FILES", 2)
    (tmp_path / "big.md").write_text("x" * 101, encoding="utf-8")
    with pytest.raises(TemplateError, match="big.md is larger than 100 bytes"):
        expand("@big.md", ctx_any(), tmp_path, [])
    (tmp_path / "many").mkdir()
    for n in range(3):
        (tmp_path / "many" / f"{n}.txt").write_text("x", encoding="utf-8")
    with pytest.raises(TemplateError, match="many/ holds more than 2 files"):
        expand("@many/", ctx_any(), tmp_path, [])


def _bad_template(tmp_path, name: str, content: bytes):
    d = tmp_path / name
    d.mkdir()
    (d / "template.yaml").write_bytes(content)
    return d


BOMB = b"a: &a [x, x, x, x, x, x, x, x, x]\n" + b"".join(
    f"{chr(98 + i)}: &{chr(98 + i)} [{', '.join([f'*{chr(97 + i)}'] * 9)}]\n".encode() for i in range(8))


@pytest.mark.parametrize("name, content, why", [
    ("latin1", "name: Caf\xe9\n".encode("latin-1"), "not UTF-8"),
    ("deep", b"x: " + b"[" * 5000 + b"]" * 5000 + b"\n", "nested deeper than"),
    ("bomb", BOMB, "more than"),
])
def test_one_unreadable_template_never_breaks_the_listing(fixtures, tmp_path, capsys, name, content, why):
    _bad_template(tmp_path, name, content)
    started = time.monotonic()
    ids = [t.id for t in list_templates([fixtures, tmp_path])]
    assert ids == ["bad-include", "bad-placeholder", "shop"]
    assert time.monotonic() - started < 5
    err = capsys.readouterr().err
    assert name in err and why in err, err


def test_a_template_that_cant_be_read_is_skipped(fixtures, tmp_path, capsys):
    d = _bad_template(tmp_path, "locked", b"id: locked\n")
    (d / "template.yaml").chmod(0)
    try:
        assert [t.id for t in list_templates([fixtures, tmp_path])] == ["bad-include", "bad-placeholder", "shop"]
    finally:
        (d / "template.yaml").chmod(0o644)
    assert "locked" in capsys.readouterr().err


@pytest.mark.parametrize("change, path", [
    ({"size.max": 100_000}, "size.max"),
    ({"names": {"first": ["A"] * 201, "last": ["B"]}}, "names"),
    ({"roles.0.count": {"fixed": 100_000}}, "roles[0].count"),
])
def test_sizes_and_name_pools_have_limits(fixtures, tmp_path, change, path):
    with pytest.raises(TemplateError) as e:
        load_template(copy_with(fixtures / "shop", tmp_path, change))
    assert e.value.path.startswith(path), e.value


def test_included_name_pools_have_the_same_limits(fixtures, tmp_path):
    dst = copy_with(fixtures / "shop", tmp_path, {})
    (dst / "names.yaml").write_text("first: [" + ", ".join(f"F{n}" for n in range(201)) + "]\nlast: [L]\n",
                                    encoding="utf-8")
    with pytest.raises(TemplateError) as e:
        load_template(dst)
    assert e.value.path.startswith("names@names.yaml.first"), e.value
