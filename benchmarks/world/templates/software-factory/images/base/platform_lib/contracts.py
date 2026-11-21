"""Consumer contracts: what a service expects of the answers of the services it calls. A contract is
`{consumer, provider, calls: [{method, path, fields: {<name>: <JSON type>}}]}`; a body meets a call when it is
an object holding each field with that type (extra fields are fine)."""


def json_type(value) -> str:
    if isinstance(value, bool):
        return "boolean"
    if isinstance(value, (int, float)):
        return "number"
    return {str: "string", list: "array", dict: "object", type(None): "null"}.get(type(value), "unknown")


def failures(call: dict, body) -> list[str]:
    """Why `body` does not meet `call`, one line per field; empty when it does."""
    where = f"{call['method']} {call['path']}"
    if not isinstance(body, dict):
        return [f"{where}: the body is {json_type(body)}, expected object"]
    out = []
    for name, expected in call["fields"].items():
        if name not in body:
            out.append(f"{where}: field {name} missing")
        elif json_type(body[name]) != expected:
            out.append(f"{where}: {name} is {json_type(body[name])}, expected {expected}")
    return out
