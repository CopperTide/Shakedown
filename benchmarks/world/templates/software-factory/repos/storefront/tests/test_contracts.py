"""What storefront tells its providers it needs (`contracts/`), held against the fakes its tests run on, so the
tests and the contracts say the same thing."""
import json
from pathlib import Path

import pytest
from fakes import FakeInventory, FakeOrders

from platform_lib.contracts import failures

CONTRACTS = sorted((Path(__file__).resolve().parents[1] / "contracts").glob("*.json"))


def fake_for(provider: str):
    inventory = FakeInventory()
    return {"inventory": inventory, "orders": FakeOrders(inventory)}[provider]


@pytest.mark.parametrize("path", CONTRACTS, ids=lambda p: p.stem)
def test_the_fakes_meet_the_contracts(path):
    import httpx
    contract = json.loads(path.read_text(encoding="utf-8"))
    assert contract["consumer"] == "storefront" and contract["provider"] == path.stem
    fake = fake_for(contract["provider"])
    for call in contract["calls"]:
        body = fake.handle(httpx.Request(call["method"], f"http://fake.test{call['path']}")).json()
        assert failures(call, body) == []


def test_there_is_a_contract_for_each_service_storefront_calls():
    assert [p.stem for p in CONTRACTS] == ["inventory", "orders"]
