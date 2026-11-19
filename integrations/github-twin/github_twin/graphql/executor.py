"""Runs a GraphQL request over the twin's schema with graphql-core: validation errors in graphql-ruby
words, the query or mutation root, and whether it wrote (`X-Arena-Effect`).

Fields resolve to the attribute or method of the field's name in snake case on the object (a dict:
the key of the field's name). A field an object lacks answers its type's empty value: null, or for a
non-null type an empty connection or list, `false`, `0`, `""`, or the enum's first value, so a client
may ask for any field the schema has."""
import re
from functools import cache
from pathlib import Path

from graphql import (GraphQLError, OperationType, OverlappingFieldsCanBeMergedRule, build_schema, execute,
                     get_named_type, get_operation_ast, is_enum_type, is_list_type, is_non_null_type, is_scalar_type,
                     parse, specified_rules, validate)

from . import errors
from .paging import empty
from .resolvers.mutations import Mutation
from .resolvers.root import Query

SCHEMA_FILE = Path(__file__).with_name("schema.graphql")
#: graphql-ruby (GitHub) lets fields of one name in fragments on different types return different
#: types (gh's `...on Issue{state} ...on PullRequest{state}`), which the spec's rule forbids.
RULES = [r for r in specified_rules if r is not OverlappingFieldsCanBeMergedRule]
_MISSING = object()


@cache
def schema():
    return build_schema(SCHEMA_FILE.read_text())


@cache
def snake(name: str) -> str:
    name = re.sub(r"([A-Z]+)([A-Z][a-z])", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", name).lower()


SCALARS = {"Boolean": False, "Int": 0, "Float": 0.0}


def empty_value(t):
    """What a field the object lacks answers (see the module's doc)."""
    if not is_non_null_type(t):
        return None
    inner = t.of_type
    if is_list_type(inner):
        return []
    if is_enum_type(inner):
        return next(iter(inner.values))
    named = get_named_type(inner)
    if named.name.endswith("Connection"):
        return empty()
    return SCALARS.get(named.name, "") if is_scalar_type(named) else None


def resolve(source, info, **args):
    name = info.field_name
    if isinstance(source, dict):
        return source[name] if name in source else empty_value(info.return_type)
    value = getattr(source, snake(name), _MISSING)
    if value is _MISSING:
        return empty_value(info.return_type)
    return value(**{snake(k): v for k, v in args.items()}) if callable(value) else value


def type_of(value, info, abstract_type):
    return value["__typename"] if isinstance(value, dict) else value.typename


def _invalid(errs: list) -> dict:
    return {"errors": [errors.invalid(e) for e in errs]}


def run(query: str, variables: dict | None, operation: str | None, gql) -> tuple[dict, str]:
    """The answer to one request and its effect: `write` when a mutation changed something."""
    try:
        doc = parse(query or "")
    except GraphQLError as e:
        return _invalid([e]), "read"
    errs = validate(schema(), doc, RULES)
    op = get_operation_ast(doc, operation) if not errs else None
    if errs or op is None:
        return _invalid(errs or [GraphQLError(f"Operation name {operation!r} not found in the document")]), "read"
    root = Mutation(gql) if op.operation == OperationType.MUTATION else Query(gql)
    result = execute(schema(), doc, root_value=root, context_value=gql, variable_values=variables,
                     operation_name=operation, field_resolver=resolve, type_resolver=type_of)
    out = {"data": result.data}
    if result.errors:
        out["errors"] = [errors.shaped(e) for e in result.errors]
    return out, "write" if gql.wrote else "read"
