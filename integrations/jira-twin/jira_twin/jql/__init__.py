"""A JQL subset (research §4.6): `parse(text) -> Query | JqlError`, `evaluate(query, records, ctx)`."""
from .errors import JqlError
from .evaluate import Context, evaluate
from .parser import Query, parse

__all__ = ["Context", "JqlError", "Query", "evaluate", "parse"]
