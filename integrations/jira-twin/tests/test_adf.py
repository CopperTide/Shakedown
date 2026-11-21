from jira_twin import adf


def doc(*paragraphs):
    return {"type": "doc", "version": 1, "content": [{"type": "paragraph", "content": list(p)} for p in paragraphs]}


def text(t):
    return {"type": "text", "text": t}


def test_paragraphs_and_hard_breaks():
    assert adf.to_adf("one\ntwo\n\nthree") == doc([text("one"), {"type": "hardBreak"}, text("two")], [text("three")])
    assert adf.to_text(doc([text("one"), {"type": "hardBreak"}, text("two")], [text("three")])) == "one\ntwo\n\nthree"


def test_mentions_round_trip():
    names = {"0123456789abcdef01234567": "Rev Two"}
    made = adf.to_adf("hi [~accountid:0123456789abcdef01234567], look", names.get)
    mention = {"type": "mention", "attrs": {"id": "0123456789abcdef01234567", "text": "@Rev Two"}}
    assert made == doc([text("hi "), mention, text(", look")])
    assert adf.to_text(made) == "hi [~accountid:0123456789abcdef01234567], look"


def test_empty_text_is_an_empty_doc():
    assert adf.to_adf("") == {"type": "doc", "version": 1, "content": []}
    assert adf.to_text({"type": "doc", "version": 1, "content": []}) == ""


def test_other_nodes_keep_their_text():
    d = {"type": "doc", "version": 1, "content": [
        {"type": "heading", "attrs": {"level": 2}, "content": [text("Title")]},
        {"type": "bulletList", "content": [{"type": "listItem", "content": [
            {"type": "paragraph", "content": [text("item"), {"type": "inlineCard", "attrs": {"url": "https://x.test"}}]}]}]},
        {"type": "codeBlock", "content": [text("x = 1")]}]}
    assert adf.to_text(d) == "Title\n\nitem https://x.test\n\nx = 1"


def test_is_doc():
    assert adf.is_doc(doc([text("x")])) and not adf.is_doc("x") and not adf.is_doc({"type": "paragraph"})
