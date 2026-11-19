"""What orders tells inventory it needs (`contracts/inventory.json`), held against the fake its tests run on, so
the tests and the contract say the same thing."""
import json
from pathlib import Path

import httpx
from fakes import RESERVATION, FakeInventory

from platform_lib.contracts import failures

CONTRACT = Path(__file__).resolve().parents[1] / "contracts" / "inventory.json"


def test_the_fake_inventory_meets_the_contract():
    contract = json.loads(CONTRACT.read_text(encoding="utf-8"))
    assert (contract["consumer"], contract["provider"]) == ("orders", "inventory")
    for call in contract["calls"]:
        fake = FakeInventory({**RESERVATION, "id": call["path"].split("/")[2]})
        body = fake.handle(httpx.Request(call["method"], f"http://fake.test{call['path']}")).json()
        assert failures(call, body) == []
