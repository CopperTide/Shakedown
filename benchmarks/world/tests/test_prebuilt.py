import pytest

from worldbench import prebuilt


def _files(root):
    """`root`'s built files and their bytes: not the kept ones, nor Python's caches (which running a template's
    code leaves in it)."""
    files = {p.relative_to(root): p for p in sorted(root.rglob("*")) if p.is_file()}
    return {rel.as_posix(): p.read_bytes() for rel, p in files.items()
            if rel.as_posix() not in prebuilt.KEPT and "__pycache__" not in rel.parts}


@pytest.mark.parametrize("id", prebuilt.PREBUILT)
def test_a_prebuilt_world_is_its_template_built_with_the_defaults(id, tmp_path):
    shipped = prebuilt.BENCHMARKS / id
    built = prebuilt.write(id, tmp_path / id)
    assert built["name"] == id
    stale = f"benchmarks/{id} is stale: run `uv run python -m worldbench.prebuilt` in benchmarks/world"
    assert _files(tmp_path / id) == _files(shipped), stale
    assert all((shipped / name).is_file() for name in prebuilt.KEPT)
