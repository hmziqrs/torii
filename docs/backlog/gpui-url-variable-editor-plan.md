# GPUI URL Variable Editor Plan

## Goal

Implement a Postman/Insomnia-style URL input for Torii on top of `gpui-component`, with:

- single-line editor behavior
- inline variable-aware highlighting
- unresolved-variable diagnostics
- autocomplete for variables and snippets
- hover details for resolved values

This should apply first to the request URL bar, then become reusable for other request inputs if the pattern holds.

## Current State

Torii already uses `gpui_component::input::InputState` for all request editors.

Relevant current files:

- `src/views/item_tabs/request_tab.rs`
- `src/views/item_tabs/request_tab/init.rs`
- `src/views/item_tabs/request_tab/layout.rs`

Today:

- the URL bar is still a plain `InputState`
- JSON body uses `code_editor("json")`
- script/test editors use `code_editor("javascript")`

The URL input is therefore missing the editor-specific machinery already available elsewhere in the request tab.

## What `gpui-component` Already Gives Us

`gpui-component`'s input editor is enough to serve as the base layer.

Useful capabilities already present:

- `InputState::code_editor(language)`
- `InputState::multi_line(false)`
- syntax highlighter state
- diagnostics via `diagnostics_mut()`
- completion provider hook via `state.lsp.completion_provider`
- hover provider hook via `state.lsp.hover_provider`
- inline completion support

This means we do not need to build a custom low-level GPUI editor just to get URL-variable behavior.

## Recommended Architecture

Treat the URL bar as a single-line code editor with Torii-specific parsing and editor providers layered on top.

### Layer 1: Editor Shell

Use `gpui-component` editor mode for the URL field:

- convert `url_input` to `InputState::new(...).code_editor(...).multi_line(false)`
- keep existing request-bar layout and focus ring container
- disable editor features that do not belong in a URL bar, such as line numbers

This gives Torii a one-line editor rather than a plain text box.

### Layer 2: URL Variable Model

Add a small Torii-local parser for URL input tokens.

Initial token types:

- plain text
- scheme and host text
- path parameters like `:id`
- template variables like `{{baseUrl}}`
- malformed template spans

This parser should be cheap, synchronous, and independent from rendering.

Suggested module:

- `src/views/item_tabs/request_tab/url_editor.rs`

Potential submodules later:

- `parser.rs`
- `completion.rs`
- `hover.rs`
- `diagnostics.rs`

### Layer 3: Resolution Context

Introduce a resolver that maps tokens to runtime meaning.

Resolution source:

- the resolver reads from the warm variable snapshot held in `WorkspaceScopeState` (to be introduced by Phase 4 Slice 2) — it does not query the variable store or `VariableResolutionService` per keypress
- the precedence order follows Phase 4: request-local overrides → active environment variables → workspace variables
- full request resolution (producing `ResolvedRequest`) remains `VariableResolutionService`'s responsibility and only runs on explicit send; the URL editor resolver is a lightweight existence check against the in-memory snapshot, not a parallel resolution pipeline

Output should be structured, not just strings.

Suggested types:

```rust
pub enum UrlTokenKind {
    Text,
    PathParam,
    Variable,
    InvalidVariable,
}

pub struct UrlToken {
    pub range: std::ops::Range<usize>,
    pub kind: UrlTokenKind,
    pub raw: String,
}

pub enum UrlResolution {
    Resolved { display_value: String },
    Missing,
    Invalid { message: String },
}
```

The parser should not know whether a variable exists. The resolver should.

## Implementation Phases

Phase 1 and Phase 2 are standalone and can proceed before Phase 4 infrastructure exists. Phases 3–5 have hard dependencies on Phase 4 slices:

| This plan | Requires Phase 4 |
|---|---|
| Phase 1 — editor mode | none |
| Phase 2 — URL parser | none |
| Phase 3 — diagnostics | Slice 0 (`VariableEntry` domain) + Slice 2 (warm variable snapshot in session scope) |
| Phase 4 — autocomplete | Slice 3 (CRUD surfaces) + Slice 5 (active environment in session) |
| Phase 5 — hover | Slice 6 (`VariableResolutionService` exists) |

Do not start Phases 3–5 before the corresponding Phase 4 slices land.

## Phase 1: Convert URL Input to Editor Mode

Change request-tab URL input initialization in `src/views/item_tabs/request_tab/init.rs`.

Target shape:

```rust
let url_input = cx.new(|cx| {
    let mut state = InputState::new(window, cx)
        .code_editor("plaintext")
        .multi_line(false);
    state.set_value(initial.url.clone(), window, cx);
    state
});
```

Notes:

- the exact language string may need adjustment depending on what the highlighter recognizes
- syntax highlighting is not the main reason for phase 1; the main reason is to switch onto the editor code path
- do not call `.line_number(false)` or `.soft_wrap(false)` on a single-line editor: `line_number` has a `debug_assert!(is_code_editor() && is_multi_line())` guard and `soft_wrap` has a `debug_assert!(is_multi_line())` guard — both panic in debug builds when `multi_line` is false; line numbers already do not render in single-line mode and soft wrap is a no-op

Success criteria:

- URL bar still behaves like a single-line input
- focus, save sync, and keyboard handling still work
- no layout regressions in the request header row

## Phase 2: Add a Torii URL Parser

Create a parser that scans the current URL text and returns token spans.

Initial syntax:

- `{{name}}` for variables
- `:id` for path params

Do not start with nested expressions or function calls.

Success criteria:

- parser is deterministic
- parser works on partial/incomplete input
- malformed spans are represented explicitly

## Phase 3: Add Diagnostics

Use `InputState::diagnostics_mut()` to surface:

- missing variable
- malformed variable syntax
- invalid path param syntax if needed

Diagnostics should be recomputed when:

- URL text changes
- variable source data changes

This gives immediate visual feedback before rich styling exists.

Success criteria:

- unresolved variables are visibly marked
- malformed `{{...` syntax is visibly marked
- updates are incremental enough to avoid render churn

## Phase 4: Add Autocomplete

Implement a local completion provider using `state.lsp.completion_provider`.

Completion sources:

- environment variables
- workspace variables
- common URL snippets such as `https://`
- path param names if route context exists

Trigger rules:

- after typing `{{`
- after typing `:`
- optionally manual completion shortcut

Completion insertions should replace only the relevant token span.

Success criteria:

- suggestions appear only in useful contexts
- accepted completion writes the expected text
- completion does not interfere with normal URL typing

## Phase 5: Add Hover

Implement a local hover provider using `state.lsp.hover_provider`.

Hover content examples:

- variable name
- resolved value preview
- source of the value
- missing-variable message

This should be informational only. Clicking and editing can come later.

Success criteria:

- hovering `{{baseUrl}}` shows its resolution state
- hovering invalid spans shows the parse or validation error

## Phase 6: Improve Highlighting

There are two possible paths for inline color differentiation.

### Option A: Ship V1 with Diagnostics + Existing Syntax Highlighting

This is the fastest route.

Use:

- default editor rendering
- diagnostics for bad spans
- completion and hover for rich behavior

This may already be good enough for a first release.

### Option B: Add Richer Variable Coloring

If diagnostics-only styling is not enough, then add one of:

- a tiny custom tree-sitter grammar for URL templates
- a local extension seam in `gpui-component` for custom highlight ranges

Do not start here.

This is phase-2 polish, not the first implementation milestone.

## Why Not Build a Custom GPUI Element First

That would duplicate editor behavior Torii already gets from `gpui-component`.

We already have:

- cursor movement
- selection
- completion menu plumbing
- hover plumbing
- diagnostics rendering
- single-line editor support

Building a custom element first would increase scope before we know what the stock editor cannot do.

## Recommended File Layout

Initial minimal layout:

- `src/views/item_tabs/request_tab/url_editor.rs`

Possible expanded layout:

- `src/views/item_tabs/request_tab/url_editor/mod.rs`
- `src/views/item_tabs/request_tab/url_editor/parser.rs`
- `src/views/item_tabs/request_tab/url_editor/resolver.rs`
- `src/views/item_tabs/request_tab/url_editor/completion.rs`
- `src/views/item_tabs/request_tab/url_editor/hover.rs`
- `src/views/item_tabs/request_tab/url_editor/diagnostics.rs`

Start with one file. Split only after the feature stabilizes.

## State Ownership

Keep URL editor behavior inside `RequestTabView` at first.

Do not introduce app-wide variable-editor infrastructure yet.

The request tab already owns:

- the URL input entity
- request draft synchronization
- request-scoped execution state

That is the correct first ownership boundary.

## Data Flow

Desired flow on URL change:

1. user edits URL text
2. request tab subscription receives `InputEvent::Change`
3. draft URL is updated
4. URL parser runs on current text and produces `Vec<UrlToken>`
5. resolver maps variable tokens against the warm variable snapshot in `WorkspaceScopeState`; no separate `cx.notify()` is emitted — this step runs synchronously inside the existing subscription handler before the handler's own `cx.notify()` fires
6. diagnostics are pushed to `InputState::diagnostics_mut()`
7. completion and hover providers read the same parsed/resolved model on demand

The resolver must be integrated into the existing `InputEvent::Change` subscription handler, which already carries a reentrancy guard and KV-sync logic. Adding a second `cx.notify()` from the resolver would create a second render cycle and risk re-entrancy. Step 5 must complete before the handler returns.

Avoid reparsing in multiple disconnected places.

## Performance Notes

Keep parsing local and cheap.

Guidelines:

- parse only the current URL text
- avoid async work for parsing
- debounce only completion/hover fetches if needed
- avoid rebuilding editor entities on every render
- update diagnostics only when text or variable sources change
- the completion and hover providers use `Rc<dyn CompletionProvider>` and `Rc<dyn HoverProvider>` — these are `Rc`, not `Arc`, which means they are single-threaded and must produce results synchronously or schedule through GPUI's async mechanisms; for the local variable source (warm in-memory snapshot) this is fine; do not design a provider that blocks on SQLite or makes a network call

This should be far cheaper than body editor behavior because the URL field is short and single-line.

## First Implementation Cut

The first shippable slice should be:

- URL bar moved to single-line editor mode
- parser for `{{var}}` and `:id`
- diagnostics for missing and malformed variables
- basic variable completion

Do not block the first version on:

- custom syntax grammar
- chips/pills inside the editor
- clickable variable widgets
- global reuse across every input in the app

## Open Questions

Resolved by Phase 4 — not open:

- "What is the authoritative source of environment/workspace variables?" — `VariableResolutionService` (Phase 4 Slice 6), precedence: request-local → active environment → workspace variables, sourced from the collection store and `WorkspaceScopeState`. The URL editor resolver reads from the same warm snapshot.
- "What exact variable syntax should Torii standardize on?" — `{{variable}}` is already standardized across the codebase (`check_unresolved_placeholders` in `request_execution.rs` checks for `{{` and `}}`).

Genuinely open:

- whether `:id` path params should be purely visual highlighting or tied to request params state; implement as visual-only until a path-params feature is explicitly added — do not connect `:id` tokens to params state in this plan
- whether URL autocomplete should suggest stored base URLs or prior hosts

## Recommendation

Proceed with the editor-based approach using `gpui-component` hooks, not a custom GPUI input from scratch. No existing completion or hover providers exist in Torii — this feature introduces the first ones.

The implementation order should be:

1. switch URL input to single-line code editor mode (standalone, no Phase 4 dependency)
2. add Torii URL parser and resolver against warm variable snapshot (standalone)
3. add diagnostics — requires Phase 4 Slice 0 + Slice 2
4. add completion provider — requires Phase 4 Slice 3 + Slice 5
5. add hover provider — requires Phase 4 Slice 6
6. revisit richer token coloring only if needed

This keeps scope controlled and matches how the major API clients solve the same problem: editor surface first, domain-specific parsing and completion on top.
