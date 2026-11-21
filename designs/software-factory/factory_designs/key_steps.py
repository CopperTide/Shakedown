"""The steps of a run that matter (``key_steps.yaml``): a merge, an approval, a revert, a deploy, an incident,
each found among the run's recorded events, with the reasoning its agent recorded just before it."""
from __future__ import annotations

from fnmatch import fnmatchcase
from pathlib import Path
from typing import Any, Optional
from urllib.parse import urlsplit

import yaml

RULES = Path(__file__).resolve().parent.parent / "key_steps.yaml"


def load_rules(path: Path = RULES) -> list[dict]:
    return yaml.safe_load(path.read_text())


def _http_matches(rule: dict, p: dict) -> bool:
    args = p.get("args") or {}
    if p.get("name") != "http" or args.get("method") != rule["method"]:
        return False
    if not fnmatchcase(urlsplit(args.get("url", "")).path, rule["path"]):
        return False
    text = (args.get("body") or {}).get("text") or ""
    return "body" not in rule or fnmatchcase(text, f"*{rule['body']}*")


def _event_matches(rule: dict, e: dict) -> bool:
    p = e["payload"]
    if "http" in rule:
        return e["kind"] == "tool_call" and _http_matches(rule["http"], p)
    if "event" in rule:
        return e["kind"] == "world_event" and p.get("kind") == rule["event"]
    args = p.get("args") or {}
    return (e["kind"] == "tool_call" and p.get("name") == rule["tool"]
            and all(args.get(k) == v for k, v in (rule.get("args") or {}).items()))


def what_of(e: dict, rules: list[dict]) -> Optional[str]:
    """The first rule's ``what`` that ``e`` matches."""
    return next((r["what"] for r in rules if _event_matches(r, e)), None)


def agent_of(e: dict) -> Optional[str]:
    """Who took the step: the recorded agent, else (a world's own event) who its data names."""
    if e.get("agent") and e["agent"] != "link":
        return e["agent"]
    return ((e["payload"].get("data") or {}).get("by")) or (e["payload"].get("args") or {}).get("person")


def key_steps(events: list[dict], rules: list[dict]) -> list[dict[str, Any]]:
    """``[{seq, agent, what, reasoning_seq}]`` in order; ``reasoning_seq`` is that agent's latest
    ``reasoning`` event before the step (None when it recorded none)."""
    out, last_reasoning = [], {}
    for e in events:
        if e["kind"] == "reasoning" and e.get("agent"):
            last_reasoning[e["agent"]] = e["seq"]
        what = what_of(e, rules)
        if what:
            agent = agent_of(e)
            out.append({"seq": e["seq"], "agent": agent, "what": what,
                        "reasoning_seq": last_reasoning.get(agent)})
    return out
