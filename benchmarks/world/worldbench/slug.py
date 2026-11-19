"""A company's name as a package name (its slug) and as a short key of capitals."""
import hashlib
import re
import unicodedata

MAX = 40
# Letters NFKD leaves whole, spelled in ASCII.
_SPELLED = str.maketrans({"Æ": "AE", "æ": "ae", "Ø": "O", "ø": "o", "Œ": "OE", "œ": "oe", "ß": "ss", "Ð": "D",
                          "ð": "d", "Þ": "TH", "þ": "th", "Ł": "L", "ł": "l", "Đ": "D", "đ": "d", "ı": "i"})


def ascii_of(text: str) -> str:
    """`text` in ASCII: accents dropped, a few letters spelled out, anything else left out."""
    decomposed = unicodedata.normalize("NFKD", text.translate(_SPELLED))
    return decomposed.encode("ascii", "ignore").decode()


def slug_of(name: str) -> str:
    """Lowercase ASCII words joined by `-`, at most 40 characters (cut at a word); `world-<6 hex>` when nothing
    is left."""
    slug = re.sub(r"[^a-z0-9]+", "-", ascii_of(name).lower()).strip("-")
    if len(slug) > MAX:
        head = slug[:MAX + 1]
        cut = head.rfind("-")
        slug = (head[:cut] if cut > 0 else slug[:MAX]).strip("-")
    return slug or "world-" + hashlib.sha256(name.encode()).hexdigest()[:6]


def key_of(name: str) -> str:
    """2–4 capitals: the initials of the name's words, or a one-word name's first two letters."""
    words = re.findall(r"[A-Za-z]+", ascii_of(name))
    if not words:
        return "WD"
    if len(words) == 1:
        return (words[0] * 2)[:2].upper()
    return "".join(w[0] for w in words[:4]).upper()
