from dataclasses import replace
from pathlib import Path

import pytest
from support import ctx_any, expand_all

from worldbench.expand import Context, expand
from worldbench.template import TemplateError, load_template


def test_an_include_outside_the_template_is_refused(fixtures):
    with pytest.raises(TemplateError, match="leaves the template folder"):
        expand_all(load_template(fixtures / "bad-include"))


def test_a_whole_string_placeholder_keeps_its_type_and_braces_escape():
    ctx = Context(company={"name": "Acme", "slug": "acme"}, size=12, options={}, people=[{"id": "p-001"}], params={})
    assert expand("{size}", ctx, Path("."), []) == 12
    assert expand("{people}", ctx, Path("."), []) == [{"id": "p-001"}]
    assert expand("{{literal}} {company.name}", ctx, Path("."), []) == "{literal} Acme"


def test_an_unknown_placeholder_is_a_template_error(fixtures):
    with pytest.raises(TemplateError, match=r"unknown placeholder \{company\.ceo\}") as e:
        expand_all(load_template(fixtures / "bad-placeholder"))
    assert e.value.path == "tasks[0].ticket"


def test_a_folder_include_is_copied_and_named_by_world_dir(tmp_path):
    (tmp_path / "repos").mkdir()
    files = []
    assert expand("@repos/", ctx_any(), tmp_path, files) == "${WORLD_DIR}/files/repos/"
    assert files == [(tmp_path / "repos", "files/repos/")]


def test_file_includes_are_parsed_or_read_and_expanded(fixtures):
    shop = fixtures / "shop"
    files = []
    assert expand("@tickets/alpha.md", ctx_any(), shop, files) == "Order 1 of Acme must ship today ({urgent}).\n"
    config = expand("@data/alpha.yaml", replace(ctx_any(), options={"alpha": {"profile": "lean"}}), shop, files)
    assert config == {"company": "Acme", "profile": "lean", "seed": "${WORLD_DIR}/files/seed/", "size": 3}
    assert files == [(shop / "seed", "files/seed/")]


def test_people_of_a_role_options_params_and_world_dir():
    people = [{"id": "p-001", "role": "a"}, {"id": "p-002", "role": "b"}]
    ctx = Context(company={"name": "Acme"}, size=2, options={"x": {"mode": "on"}}, people=people, params={"n": 3})
    assert expand("{people.b}", ctx, Path("."), []) == [people[1]]
    assert expand("{options.x.mode}/{param.n}", ctx, Path("."), []) == "on/3"
    assert expand({"d": "{world_dir}/x"}, ctx, Path("."), []) == {"d": "${WORLD_DIR}/x"}


def test_errors_name_the_path_inside_lists_and_includes(tmp_path):
    (tmp_path / "a.yaml").write_text("k: [ok, '{nope}']\n")
    with pytest.raises(TemplateError) as e:
        expand({"x": ["@a.yaml"]}, ctx_any(), tmp_path, [])
    assert e.value.path == "x[0]@a.yaml.k[1]"


def test_a_missing_include_and_a_list_inside_text_are_errors(tmp_path):
    with pytest.raises(TemplateError, match="no file none.md"):
        expand("@none.md", ctx_any(), tmp_path, [])
    with pytest.raises(TemplateError, match=r"\{people\} is a list: use it as the whole string"):
        expand("all: {people}", ctx_any(), tmp_path, [])


def test_a_string_with_another_at_sign_stays_text(tmp_path):
    assert expand("@alice", ctx_any(), tmp_path, []) == "@alice"
