"""Tool parameters: an array says what its items are, in the schema models read, and calls are checked against it."""
import pytest

from shakedown_part.params import Param, Refusal, input_schema, normalize


def test_an_array_names_its_item_type_in_the_schema():
    params = (Param("flights", "array", items="string"),
              Param("offers", "array", None, items={"type": "object", "properties": {"group": {"type": "string"}},
                                                   "required": ["group"]}),
              Param("tags", "array", None))
    props = input_schema(params)["properties"]
    assert props["flights"] == {"type": "array", "items": {"type": "string"}}
    assert props["offers"]["items"]["required"] == ["group"]
    assert props["tags"] == {"type": "array"}, "without items, as before"
    assert input_schema(params)["required"] == ["flights"]


def test_items_are_checked():
    p = (Param("flights", "array", items="string"),)
    assert normalize("t", p, {"flights": ["a", "b"]}) == {"flights": ["a", "b"]}
    with pytest.raises(Refusal, match=r"flights\[1\] must be of type string"):
        normalize("t", p, {"flights": ["a", 3]})
    with pytest.raises(Refusal, match="flights must be of type array"):
        normalize("t", p, {"flights": "a"})
    assert normalize("t", (Param("x", "array", None, items="string"),), {"x": None}) == {"x": None}
