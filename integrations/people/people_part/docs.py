"""The people part's documents: each person (what they still mean to do and how much they did), each role's next
scheduled slots, and the log of every act."""

def docs(world, state: dict) -> dict:
    epoch = state["epoch_ms"]
    out = {f"person/{p['id']}": _person(p, state, epoch) for p in world.people}
    for role in world.roles():
        next_ = sorted(({"rule": key.split("/", 1)[1], "at_ms": epoch + at} for key, at in state["slots"].items()
                        if key.split("/", 1)[0] == role), key=lambda x: (x["at_ms"], x["rule"]))
        out[f"role/{role}"] = {"people": [p["id"] for p in world.of_role(role)], "next": next_}
    out["log/acts"] = {"entries": list(state["log"])}
    return out


def _person(p: dict, state: dict, epoch: int) -> dict:
    mine = sorted((x for x in state["pending"] if x["id"] == p["id"]), key=lambda x: (x["due_ms"], x["seq"]))
    return {"name": p["name"], "role": p["role"], "login": p["login"],
            "pending": [{"rule": x["rule"], "at_ms": epoch + x["due_ms"]} for x in mine],
            "acted": state["acted"].get(p["id"], 0), "skipped": state["skipped"].get(p["id"], 0)}


