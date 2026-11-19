"""JQL tokens: quoted strings, words (field names, values, `-7d`, `AC-12`, `cf[10020]`), operators and
punctuation. Keywords are words compared without case."""
import re
from dataclasses import dataclass

from .errors import syntax

OPERATORS = ("!=", "!~", ">=", "<=", "=", "~", ">", "<")
PUNCT = "(),"
WORD = re.compile(r"[\w.@:+\-/\[\]*]+")


@dataclass(frozen=True)
class Token:
    kind: str     # "word", "string", "op", "punct", "end"
    text: str
    pos: int

    def is_word(self, *words: str) -> bool:
        return self.kind == "word" and self.text.lower() in words


def _string(text: str, pos: int) -> tuple[Token, int]:
    quote, i, out = text[pos], pos + 1, []
    while i < len(text) and text[i] != quote:
        if text[i] == "\\" and i + 1 < len(text):
            i += 1
        out.append(text[i])
        i += 1
    if i >= len(text):
        raise syntax(f"The quoted string starting at character {pos + 1} is not closed.")
    return Token("string", "".join(out), pos), i + 1


def _one(text: str, pos: int) -> tuple[Token, int]:
    ch = text[pos]
    if ch in "\"'":
        return _string(text, pos)
    if ch in PUNCT:
        return Token("punct", ch, pos), pos + 1
    op = next((o for o in OPERATORS if text.startswith(o, pos)), None)
    if op:
        return Token("op", op, pos), pos + len(op)
    m = WORD.match(text, pos)
    if m is None:
        raise syntax(f"The character '{ch}' at position {pos + 1} is a reserved JQL character.")
    return Token("word", m.group(0), pos), m.end()


def tokens(text: str) -> list[Token]:
    out, pos = [], 0
    while pos < len(text):
        if text[pos].isspace():
            pos += 1
            continue
        tok, pos = _one(text, pos)
        out.append(tok)
    out.append(Token("end", "", len(text)))
    return out
