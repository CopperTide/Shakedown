# Assistant Implementation Plan (4)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An Ask drawer on every page lets a person do anything Shakedown does by chat. The assistant answers from the same operations as the MCP server, on the user's own model. Reads run right away; edits come back as diff cards to apply; anything that starts runs, spends money or deletes comes back as a confirm card.

**Architecture:**
- `POST /api/assistant/chat` runs a bounded tool loop (at most 8 model turns). It calls `Provider::send` with the registry operations as OpenAI-style function tools.
- Each tool call is authorized like its route. Read operations run.
- An Edit operation runs with `confirm: false`, so the project's `edit: diff_first` turns it into a proposal; `apply_then_tell` writes.
- Spend and Delete operations never run inside the loop; they become confirm cards.
- The answer carries the reply, the tools used and the cards. The UI applies a card by calling that operation's HTTP route itself.

**Tech Stack:** Rust (shaked, shakedown-project), React UI, the test fake model (`tests/support/fake_llm.rs`).

**Spec:** `docs/superpowers/specs/2026-11-20-primitives-and-editors-design.md` §13 (assistant and MCP).

## Global Constraints

- Plan 1's code-quality rules apply: clippy `-D warnings`, fmt, Rust functions of 30 lines or fewer and TS of 40, no unwrap outside tests, typed errors. UI lint at zero warnings, build, `npm test`.
- Every call goes to a real model on the user's own provider config (`providers::keys::config_for`). Tests use the fake model only.
- Spend and Delete never run without a person's click. Edits write only under `apply_then_tell` or after Apply.
- The assistant's tool calls are authorized exactly like the tools' HTTP routes (`api::authorize`), with the caller's token.
- Credentials never appear in prompts: tool results come from operations, which never return secret values.

## Review Focus

1. **A model that loops on tools.** The loop stops after 8 turns with "Stopped after 8 steps" (Task 2 test `stops_after_eight_turns`).
2. **No model configured.** 409 "The assistant needs a model: add a key in Connections & models" (Task 2 test `no_model_is_explained`).
3. **An unknown tool name from the model.** It becomes a tool error message back to the model, not a crash (Task 2 test `unknown_tool_is_a_tool_error`).
4. **A read-only token.** Its edit cards can't be applied (the route refuses), and the assistant still answers (Task 3 test `read_token_gets_cards_not_writes`).
5. **The drawer opened on a page with no project.** It still opens and shows the model's answer (Task 4 test `drawer_without_project`).

---

### Task 1: Assistant settings in the project file

**Files:** Modify `core/crates/project/src/format/project.rs`, adding `assistant: Option<AssistantSettings { model: String, provider: Option<String> }>`. Test: `core/crates/project/tests/formats.rs` (`assistant_settings_round_trip`).

- [ ] **Step 1:** Write the failing test.
- [ ] **Step 2:** Run it. Expected: FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run it. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(project): assistant settings`

### Task 2: The chat loop

**Files:**
- Create: `core/crates/shaked/src/assistant/mod.rs` (`ChatRequest`, `ChatAnswer`, `Card`, `run_chat`), `assistant/tools.rs` (the operations as function tools, and the dispatch of one tool call to `Ran | Card | Error`), `assistant/prompt.rs` (the system prompt from the page and the project's name)
- Create: `core/crates/shaked/src/api/assistant.rs`, with the route `POST /api/assistant/chat` and a ROUTES row (`Read` on `Instance`)
- Test: `core/crates/shaked/tests/assistant.rs`

**Interfaces:**
- `ChatRequest { messages: Vec<Value> /* {role, content} */, page: Option<String> }`
- `ChatAnswer { reply: String, messages: Vec<Value> /* the conversation to send back next time */, tools: Vec<ToolUse { name, summary }>, cards: Vec<Card> }`
- `Card::Diff { tool, arguments, diff }` and `Card::Confirm { tool, arguments, access }`
- `run_chat(ctx: &OpCtx<'_>, provider: &Provider, model: &str, req: ChatRequest) -> Result<ChatAnswer, OpError>`
- The model comes from the project's `assistant.model`, else the provider's `default_model()`. The provider comes from `assistant.provider`, else the configured default. When none is available, the answer is 409 with the Review Focus message.

- [ ] **Step 1: Write the failing tests,** with `fake_llm` scripting OpenAI answers:
  - `reads_run_and_answer`: first a tool call `worlds_list`, then the text "One world." The reply is "One world.", `tools` names `worlds_list`, and the first request's `tools` include `scenarios_put`.
  - `edit_becomes_a_diff_card`: a tool call `scenarios_put` with a changed label. The card is `Diff` with `+label:`, and the file is unchanged.
  - `spend_becomes_a_confirm_card`: a tool call `experiments_deploy`. The card is `Confirm`, and `deployments_of` stays empty.
  - `unknown_tool_is_a_tool_error`
  - `stops_after_eight_turns`
  - `no_model_is_explained`
- [ ] **Step 2:** Run `cargo test -p shaked --test assistant`. Expected: FAIL.
- [ ] **Step 3: Implement,** with small functions: `tool_specs()`, `dispatch()`, `one_turn()`, `run_chat()`.
- [ ] **Step 4:** Run the tests, routes, parity and clippy. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(shaked): the assistant's chat loop over the operation registry`

### Task 3: Applying cards safely

**Files:** Test `core/crates/shaked/tests/assistant.rs`.

- [ ] **Step 1: Write the tests:**
  - `apply_diff_card_writes`: putting the card's arguments with `confirm: true` through the route writes the file.
  - `read_token_gets_cards_not_writes`: a read token gets the card, and applying it is 403.
- [ ] **Step 2:** Run them. Expected: PASS once Task 2 is done. If they fail, fix the code, not the test.
- [ ] **Step 3: Commit:** `test(shaked): assistant cards apply only through the guarded routes`

### Task 4: The Ask drawer

**Files:**
- Create: `ui/src/api/assistant.ts`, `ui/src/features/assistant/AskDrawer.tsx`, `assistant/Message.tsx`, `assistant/DiffCard.tsx`, `assistant/ConfirmCard.tsx`, `assistant/applyCard.ts` (a card's tool name → its operation's HTTP call)
- Modify: `ui/src/features/shell/AppShell.tsx` (mount the drawer; open it on `ASK_EVENT` and ⌘J)
- Test: `ui/src/features/assistant/assistant.test.tsx`

**Behaviour:**
- The drawer sits on the right at 440px.
- **Header:** "Assistant", the model name and "Looking at <page>".
- **Messages:** user bubbles; the assistant's text; a collapsed "Used N tools" list; and cards.
- **Diff card:** "Edit <kind> <id>", the diff, then Apply (calls `putItem(kind, id, body, true)`) and Discard.
- **Confirm card:** the tool's summary and its arguments, then Confirm (POST to the operation's route) and Cancel. The card reads "Needs your OK".
- **Footer:** the composer and the note "Reads run right away. File edits show a diff to apply. Anything that starts runs or spends money asks first."

- [ ] **Step 1: Write the failing tests:**
  - `ask_event_opens_drawer`
  - `sends_page_and_shows_reply`
  - `diff_card_apply_puts`
  - `confirm_card_posts_only_on_confirm`
  - `drawer_without_project`
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** Run `npm test`, lint and build. Expected: PASS.
- [ ] **Step 5: Commit:** `feat(ui): the Ask drawer on every page`
