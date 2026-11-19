"""The arena-part/1 integration: Factory-01's ERP, a config and one JSON state per instance. A call over a
hard limit is refused with a structured denial, recorded, and reported to the world as a `denial` event."""
from arena_part import disruptions
from arena_part.jsonstate import JsonStatePart, denials_doc

from . import disrupt, erp
from .tools import TOOLS

VERSION = "0.2.0"


class Factory01Erp(JsonStatePart):
    NAME = "factory-01-erp"
    TOOLS = TOOLS
    DISRUPTIONS = disrupt.KINDS
    WORLD_TOOLS = frozenset({"erp_scenario"})

    def describe_fields(self):
        return {"version": VERSION, "fingerprint": {"adapter": VERSION},
                "system": {"kind": "erp", "product": "Simulated ERP (Factory-01)",
                           "stands_in_for": "SAP S/4HANA, Oracle (inventory, MRP, purchasing)",
                           "simulated": True}}

    def new_state(self, config):
        return erp.new_state(config)

    def docs(self, config, state):
        out = {"scenario/current": {"name": state["scenario"]},
               "purchase/orders": {"orders": state["purchases"], "limit_usd": config["po_limit_usd"]},
               "flags/all": {"flags": state["flags"]}, "log/actions": {"entries": state["log"]},
               **denials_doc(state)}
        out.update({f"line/{ln['id']}": ln for ln in config["lines"]})
        out.update({f"order/{o['id']}": erp.order_doc(config, state, o["id"]) for o in config["orders"]})
        out.update({f"item/{i['id']}": erp.item_doc(config, state, i["id"]) for i in config["items"]})
        out.update({f"supplier/{s['id']}": erp.supplier_doc(config, state, s["id"])
                    for s in config["suppliers"]})
        out.update(disruptions.docs(state))
        return out
