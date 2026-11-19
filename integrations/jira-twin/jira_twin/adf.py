"""Text and the Atlassian Document Format from one store. The twin keeps rich text as Jira's plain
(wiki) text; API v3 answers and takes ADF. Paragraphs are split by blank lines, line breaks become
`hardBreak`, and a mention `[~accountid:X]` (or Server's `[~name]`) becomes a `mention` node."""
import re

MENTION = re.compile(r"\[~(?:accountid:)?([^\]\s]+)\]")
BLOCK_JOIN = "\n\n"


def is_doc(value) -> bool:
    return isinstance(value, dict) and value.get("type") == "doc" and isinstance(value.get("content", []), list)


def _inline(line: str, names) -> list:
    out, pos = [], 0
    for m in MENTION.finditer(line):
        if m.start() > pos:
            out.append({"type": "text", "text": line[pos:m.start()]})
        name = names(m.group(1)) if names else None
        out.append({"type": "mention", "attrs": {"id": m.group(1), "text": f"@{name or m.group(1)}"}})
        pos = m.end()
    if pos < len(line):
        out.append({"type": "text", "text": line[pos:]})
    return out


def _paragraph(block: str, names) -> dict:
    content = []
    for i, line in enumerate(block.split("\n")):
        if i:
            content.append({"type": "hardBreak"})
        content += _inline(line, names)
    return {"type": "paragraph", "content": content}


def to_adf(text: str | None, names=None) -> dict:
    """`names(id)` gives a mentioned user's display name (None: the id is shown)."""
    blocks = [b for b in re.split(r"\n\s*\n", text or "") if b.strip()]
    return {"type": "doc", "version": 1, "content": [_paragraph(b, names) for b in blocks]}


def _inline_text(node: dict) -> str:
    kind = node.get("type")
    if kind == "text":
        return node.get("text", "")
    if kind == "hardBreak":
        return "\n"
    if kind == "mention":
        return f"[~accountid:{node.get('attrs', {}).get('id', '')}]"
    if kind in ("inlineCard", "blockCard"):
        return node.get("attrs", {}).get("url", "")
    if kind in ("emoji", "status", "date"):
        return node.get("attrs", {}).get("text", "")
    return _children(node, "")


def _children(node: dict, joiner: str) -> str:
    return joiner.join(_block_text(c) if c.get("type") in BLOCKS else _inline_text(c)
                       for c in node.get("content", []) if isinstance(c, dict))


def _block_text(node: dict) -> str:
    if node.get("type") in ("paragraph", "heading", "codeBlock"):
        parts = [_inline_text(c) for c in node.get("content", []) if isinstance(c, dict)]
        return _spaced(parts)
    return _children(node, BLOCK_JOIN)


def _spaced(parts: list[str]) -> str:
    """Inline pieces joined; a card next to text gets a space, as it reads in Jira."""
    out = ""
    for p in parts:
        if out and p.startswith("http") and not out.endswith((" ", "\n")):
            out += " "
        out += p
    return out


BLOCKS = {"paragraph", "heading", "codeBlock", "bulletList", "orderedList", "listItem", "blockquote", "panel",
          "table", "tableRow", "tableCell", "tableHeader", "rule", "mediaSingle", "expand"}


def to_text(doc) -> str:
    """An ADF document as the twin's text (a string is already text)."""
    if isinstance(doc, str):
        return doc
    if not is_doc(doc):
        return ""
    return BLOCK_JOIN.join(t for t in (_block_text(c) for c in doc.get("content", []) if isinstance(c, dict)) if t)
