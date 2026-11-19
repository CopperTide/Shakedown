"""The error a template is refused with: where in it, and why."""


class TemplateError(Exception):
    """A template that can't be used: `path` names the place in it (`systems[1].parts[0].config`)."""

    def __init__(self, path: str, message: str):
        super().__init__(f"{path}: {message}" if path else message)
        self.path = path
        self.message = message


def key_path(path: str, key) -> str:
    """`path` extended by a dict key (`a.b`) or a list index (`a[0]`)."""
    if isinstance(key, int):
        return f"{path}[{key}]"
    return f"{path}.{key}" if path else str(key)
