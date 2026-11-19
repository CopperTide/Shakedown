import pytest
from support import copy_with

from worldbench.template import TemplateError, form, list_templates, load_template


def test_a_good_template_loads_and_its_form_lists_what_the_ui_shows(fixtures):
    t = load_template(fixtures / "shop")
    f = form(t)
    assert [s["id"] for s in f["systems"]] == ["alpha", "beta", "gamma"]
    assert f["systems"][0]["required"] is True and f["systems"][2]["default"] is False
    assert f["size"] == {"label": "Staff", "default": 10, "min": 2, "max": 40, "presets": [5, 10, 20]}


def test_the_form_has_defaults_filled_and_no_parts(fixtures):
    f = form(load_template(fixtures / "shop"))
    assert (f["id"], f["version"], f["company"]) == ("shop", "1.0.0", {"name": "Example Co"})
    gamma = f["systems"][2]
    assert gamma == {"id": "gamma", "label": "Gamma", "product": "Gamma twin (simulated)", "required": False,
                     "default": False, "requires": ["beta"], "memory_mb": 200, "hosts": [], "options": []}
    assert f["systems"][1]["summary"] == "The second system"
    assert f["roles"][1] == {"id": "lead", "label": "Leads", "count": {"fixed": 1}, "min": 1, "systems": ["beta"]}
    assert f["events"][0]["default"] is False and f["events"][1]["needs"] == ["beta"]


def test_schema_errors_name_their_path(fixtures, tmp_path):
    broken = copy_with(fixtures / "shop", tmp_path, {"systems.0.id": "Not An Id"})
    with pytest.raises(TemplateError) as e:
        load_template(broken)
    assert e.value.path == "systems[0].id"


@pytest.mark.parametrize("change, path, message", [
    ({"systems.1.id": "alpha"}, "systems[1].id", "alpha is used twice"),
    ({"systems.2.requires": ["delta"]}, "systems[2].requires[0]", "no system delta"),
    ({"tasks.0.needs": ["delta"]}, "tasks[0].needs[0]", "no system delta"),
    ({"roles.0.systems": ["delta"]}, "roles[0].systems[0]", "no system delta"),
    ({"roles.1.rules": "rules/none.yaml"}, "roles[1].rules", "no file rules/none.yaml"),
    ({"size.presets": [1, 10]}, "size.presets[0]", "1 is outside 2..40"),
    ({"size.default": 41}, "size.default", "41 is outside 2..40"),
    ({"systems.1.parts.0.part": "alpha"}, "systems[1].parts[0].part", "alpha is used twice"),
])
def test_cross_checks_name_their_path(fixtures, tmp_path, change, path, message):
    with pytest.raises(TemplateError) as e:
        load_template(copy_with(fixtures / "shop", tmp_path, change))
    assert (e.value.path, e.value.message) == (path, message)


def test_a_template_without_template_yaml_is_an_error(tmp_path):
    with pytest.raises(TemplateError, match="no template.yaml"):
        load_template(tmp_path)


def test_list_templates_sorts_by_id_and_skips_bad_ones(fixtures, tmp_path, capsys):
    copy_with(fixtures / "shop", tmp_path, {"id": "Bad Id"})
    ids = [t.id for t in list_templates([fixtures, tmp_path, tmp_path / "missing"])]
    assert ids == ["bad-include", "bad-placeholder", "shop"]
    assert "shop" in capsys.readouterr().err
