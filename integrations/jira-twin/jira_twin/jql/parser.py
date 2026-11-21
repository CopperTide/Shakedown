"""JQL to a tree: `or` of `and` of `not`/clauses, then `ORDER BY`. A clause is `field op value`, where
the value is a word, a string, a function call, a list `( … )`, or `EMPTY`/`NULL`."""
from dataclasses import dataclass, field

from .errors import JqlError, syntax
from .lexer import Token, tokens

LIST_OPS = ("in", "not in")


@dataclass(frozen=True)
class Func:
    name: str
    args: tuple


@dataclass(frozen=True)
class Clause:
    field: str
    op: str          # = != ~ !~ > >= < <= in "not in" is "is not"
    value: object    # str, Func, tuple of those, or None for EMPTY


@dataclass(frozen=True)
class Bool:
    op: str          # and, or, not
    parts: tuple


@dataclass
class Query:
    where: object = None
    order: list = field(default_factory=list)


class _Parser:
    def __init__(self, text: str):
        self.toks, self.i = tokens(text), 0

    @property
    def tok(self) -> Token:
        return self.toks[self.i]

    def take(self) -> Token:
        t = self.toks[self.i]
        self.i += 1
        return t

    def expect(self, what: str, ok: bool) -> None:
        if not ok:
            got = f"'{self.tok.text}'" if self.tok.kind != "end" else "the end of the query"
            raise syntax(f"Expecting {what} but got {got}.")

    def query(self) -> Query:
        q = Query()
        if self.tok.kind != "end" and not self.tok.is_word("order"):
            q.where = self.or_()
        if self.tok.is_word("order"):
            q.order = self.order()
        self.expect("either 'OR' or 'AND'", self.tok.kind == "end")
        return q

    def or_(self):
        parts = [self.and_()]
        while self.tok.is_word("or"):
            self.take()
            parts.append(self.and_())
        return parts[0] if len(parts) == 1 else Bool("or", tuple(parts))

    def and_(self):
        parts = [self.not_()]
        while self.tok.is_word("and"):
            self.take()
            parts.append(self.not_())
        return parts[0] if len(parts) == 1 else Bool("and", tuple(parts))

    def not_(self):
        if self.tok.is_word("not"):
            self.take()
            return Bool("not", (self.not_(),))
        if self.tok.text == "(" and self.tok.kind == "punct":
            self.take()
            inner = self.or_()
            self.expect("')'", self.tok.text == ")")
            self.take()
            return inner
        return self.clause()

    def clause(self) -> Clause:
        self.expect("a field name", self.tok.kind in ("word", "string") and not self.tok.is_word("and", "or"))
        name = self.take().text
        op = self.operator()
        if op in LIST_OPS:
            return Clause(name, op, self.list_or_func())
        if op in ("is", "is not"):
            self.expect("EMPTY", self.tok.is_word("empty", "null"))
            self.take()
            return Clause(name, op, None)
        return Clause(name, op, self.value())

    def operator(self) -> str:
        t = self.tok
        if t.kind == "op":
            return self.take().text
        if t.is_word("in"):
            self.take()
            return "in"
        if t.is_word("not"):
            self.take()
            self.expect("'IN'", self.tok.is_word("in"))
            self.take()
            return "not in"
        if t.is_word("is"):
            self.take()
            if self.tok.is_word("not"):
                self.take()
                return "is not"
            return "is"
        self.expect("operator", False)

    def value(self):
        t = self.tok
        self.expect("either a value, list or function", t.kind in ("word", "string"))
        self.take()
        if t.kind == "word" and t.text.lower() in ("empty", "null"):
            return None
        if t.kind == "word" and self.tok.text == "(" and self.tok.kind == "punct":
            return self.func(t.text)
        return t.text

    def func(self, name: str) -> Func:
        self.take()
        args = []
        while self.tok.text != ")":
            self.expect("a function argument", self.tok.kind in ("word", "string"))
            args.append(self.take().text)
            if self.tok.text == ",":
                self.take()
        self.take()
        return Func(name, tuple(args))

    def list_or_func(self):
        if not (self.tok.text == "(" and self.tok.kind == "punct"):
            value = self.value()
            self.expect("a list or a function", isinstance(value, Func))
            return value
        self.take()
        items = [self.value()]
        while self.tok.text == ",":
            self.take()
            items.append(self.value())
        self.expect("')'", self.tok.text == ")")
        self.take()
        return tuple(items)

    def order(self) -> list:
        self.take()
        self.expect("'by'", self.tok.is_word("by"))
        self.take()
        out = [self.sort()]
        while self.tok.text == ",":
            self.take()
            out.append(self.sort())
        return out

    def sort(self) -> tuple[str, str]:
        self.expect("a field name", self.tok.kind in ("word", "string"))
        name = self.take().text.lower()
        direction = "asc"
        if self.tok.is_word("asc", "desc"):
            direction = self.take().text.lower()
        return name, direction


def parse(text: str) -> Query | JqlError:
    try:
        return _Parser(text or "").query()
    except JqlError as e:
        return e
