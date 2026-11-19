from pathlib import Path

import pytest

from factory_designs.prompts import load_prompt
from factory_designs.settings import SHARED, Settings, load_settings


def test_shared_settings_load_from_the_file():
    s = load_settings(env={})
    assert s == Settings(provider="local:runpod", model="openai/gpt-oss-120b",
                         reasoning_effort="medium", max_tokens=4096, temperature=1.0,
                         tool_output_chars=8000, command_timeout_s=600,
                         workbench_image="shakedown-workbench:1")
    assert SHARED.name == "shared.yaml"


def test_settings_env_overrides_only_provider_and_model():
    env = {"FACTORY_PROVIDER": "openrouter", "FACTORY_MODEL": "openai/gpt-oss-20b",
           "FACTORY_MAX_TOKENS": "99", "FACTORY_TEMPERATURE": "0", "FACTORY_REASONING_EFFORT": "high"}
    s = load_settings(env=env)
    assert (s.provider, s.model) == ("openrouter", "openai/gpt-oss-20b")
    base = load_settings(env={})
    assert (s.max_tokens, s.temperature, s.reasoning_effort) == (
        base.max_tokens, base.temperature, base.reasoning_effort)


def test_a_settings_file_missing_a_field_names_it(tmp_path: Path):
    p = tmp_path / "s.yaml"
    p.write_text(SHARED.read_text().replace("max_tokens: 4096\n", ""))
    with pytest.raises(ValueError, match="max_tokens"):
        load_settings(p, env={})


def test_prompt_values_are_filled_in(tmp_path: Path):
    p = tmp_path / "p.md"
    p.write_text('You have {world_minutes} minutes. Answer {"items": []}.')
    assert load_prompt(p, world_minutes="60") == 'You have 60 minutes. Answer {"items": []}.'


def test_prompt_with_a_missing_value_names_it(tmp_path: Path):
    p = tmp_path / "p.md"
    p.write_text("Ask {role_ids} within {world_minutes} minutes.")
    with pytest.raises(KeyError, match="role_ids"):
        load_prompt(p, world_minutes="60")
