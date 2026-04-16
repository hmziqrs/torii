# Tab Focus Fix — KV Editor

> Investigation findings: `docs/ui-cleanup.md` §4.6, conversation 2026-04-13
> Scope: Tab key focus navigation inside the KV editor rows

## 1. The Problem

Pressing Tab inside a KV editor input (key cell or value cell) does nothing.
The underlying chain was traced in full:

1. `"Input"` context binds `tab → IndentInline`. For single-line inputs
   `is_indentable()` returns `false`, so the action calls `cx.propagate()` and exits.
2. `"DataTable"` context binds `tab → SelectNextColumn`. This action handler
   does **not** call `cx.propagate()` — it silently updates internal column-selection
   state and returns. The event is consumed here.
3. `"Root"` context binds `tab → Tab → window.focus_next(cx)`. Because step 2
   consumed the event, this binding is **never reached**.

Result: pressing Tab inside any KV input appears to do nothing. The focus handle
remains on the current input.

---

## 2. The Goal

Inside a KV editor table, Tab should advance focus in a predictable loop:

```
Row 0: [key input] → [value input]
         ↑                 ↓
Row N: [key input] ← [value input]
```

Tab from the last value input wraps to the first key input (or adds a new row —
designer's choice).

---

## 3. Option A — Replace DataTable with Plain Rows

**What changes**
Remove the `DataTable` / `KvTableDelegate` entirely. Render KV rows as a hand-built
`v_flex` of `h_flex` rows — one per entry, containing a `Checkbox`, two `Input`
elements (key + value), and a Remove button.

```rust
// Instead of DataTable, render manually:
v_flex()
    .children(rows.iter().map(|row| {
        h_flex()
            .gap_2()
            .child(Checkbox::new(...))
            .child(Input::new(&row.key_input).flex_1())
            .child(Input::new(&row.value_input).flex_1())
            .child(Button::new(...).ghost().icon(IconName::Close))
    }))
    .child(/* Add row button */)
```

**Why Tab works after this change**
There is no `"DataTable"` ancestor key context. The binding list for Tab becomes
`[IndentInline ("Input"), Tab ("Root")]`. `IndentInline` propagates,
`window.focus_next(cx)` fires. GPUI's tab stop map cycles through all tab-stoppable
elements in the window in render order — key input → value input → next row key input
→ ... → eventually wraps to a distant element.

**Pros**
- Simple to implement (remove delegate, add a loop).
- No gpui-component modification required.
- Solves the "Tab does nothing" symptom immediately.
- Also fixes the KV table height issue (dynamic, not fixed 200 px).
- Also simplifies the input styling (no DataTable cell borders to fight).

**Cons**
- Tab cycles window-globally — after the last value input, focus moves to whatever
  the next rendered tab stop is (likely the Add Row button, then section tab buttons,
  etc.), not back to the first key input.
- Loses DataTable's column-resizing header (not needed for KV editing).
- The Tab destination after the last cell is unpredictable without additional work.

**Verdict**: Good baseline. Fixes the hard break. Tab order within a single row
(key → value) will work correctly because GPUI renders them left-to-right in order.
Cross-row Tab (value of row N → key of row N+1) will also work because they render
in DOM order. End-of-table Tab destination is uncontrolled.

---

## 4. Option B — Plain Rows + Explicit Focus Handles

**What changes**
Same as Option A (remove DataTable), but additionally give each `KvDelegateRow`
an `on_key_down` listener or a `tab_index` so the Tab order is explicit and
controlled, including the end-of-table wrap.

Specifically: store all key/value `FocusHandle` pairs in order. In the view, add a
`key_context("KvEditor")` div wrapping the rows and bind a custom `KvTabForward`
action in that context. The action handler walks `kv_rows` to find which input is
currently focused and explicitly calls `window.focus(&next_handle, cx)`.

```rust
// In kv_editor.rs render wrapper:
div()
    .key_context("KvEditor")
    .on_action(cx.listener(RequestTabView::handle_kv_tab_forward))
    .on_action(cx.listener(RequestTabView::handle_kv_tab_backward))
    .children(/* rows */)

// In request_tab.rs:
fn handle_kv_tab_forward(&mut self, _: &KvTabForward, window: &mut Window, cx: &mut Context<Self>) {
    let rows = &self.params_rows; // or headers, etc. — whichever is active
    // find which input is focused; move to next
    for (i, row) in rows.iter().enumerate() {
        if row.key_input.read(cx).focus_handle.is_focused(window) {
            window.focus(&row.value_input.read(cx).focus_handle, cx);
            return;
        }
        if row.value_input.read(cx).focus_handle.is_focused(window) {
            let next = rows.get(i + 1);
            if let Some(next_row) = next {
                window.focus(&next_row.key_input.read(cx).focus_handle, cx);
            } else {
                window.focus(&rows[0].key_input.read(cx).focus_handle, cx);
            }
            return;
        }
    }
}
```

The key binding would be registered in `app::init` alongside the other component
key bindings:
```rust
cx.bind_keys([
    KeyBinding::new("tab", KvTabForward, Some("KvEditor")),
    KeyBinding::new("shift-tab", KvTabBackward, Some("KvEditor")),
]);
```

**Why Tab works here**
The context stack for a KV input becomes `["Input", "KvEditor", "Root"]`.
Binding order: `[IndentInline, KvTabForward, Tab]`.

1. `IndentInline` fires → propagates.
2. `KvTabForward` fires → calls `window.focus(next_handle)` → does **not** propagate → consumed.
3. `Tab (Root)` never reached.

This bypasses both the DataTable problem and the window-global cycling problem.

**Pros**
- Precise control: key → value → next key → ... → wrap to row 0 key.
- Works correctly for all KV targets (Params, Headers, UrlEncoded, FormDataText).
- Shift-Tab for backward cycling is symmetric.

**Cons**
- More code: two action types, two handlers, one per active KV target
  (or a shared handler that reads `self.active_section` to pick the right row list).
- The `FocusHandle` for each input is on `InputState`, accessed via
  `row.key_input.read(cx).focus_handle` — this is internal to `InputState`, need to
  verify it is `pub` or accessible via a method.
- Must track which KV target is active when Tab is pressed.

**Verdict**: The cleanest UX. Recommended approach if Option A's end-of-table
wrapping feels wrong in practice.

---

## 5. Option C — Patch gpui-component DataTable to Propagate Tab

**What changes**
In `/Users/hmziq/os/gpui-component/crates/ui/src/table/state.rs`,
`action_select_next_col`, add a `cx.propagate()` call when column selection is
disabled:

```rust
pub(super) fn action_select_next_col(
    &mut self,
    _: &SelectNextColumn,
    _: &mut Window,
    cx: &mut Context<Self>,
) {
    // If col selection is disabled, don't handle Tab — let it bubble to Root
    if !self.col_selectable && !self.selection_mode.is_cell() {
        cx.propagate();
        return;
    }
    // ... existing logic ...
}
```

After this patch, the binding list `[IndentInline, SelectNextColumn, Tab]` becomes:
1. `IndentInline` → propagates.
2. `SelectNextColumn` → col_selectable == false → propagates.
3. `Tab (Root)` → `window.focus_next(cx)` → fires.

**Pros**
- One-line patch to gpui-component.
- Keeps DataTable UI (column headers, bordered cells, resizing).
- No restructuring of the KV editor.

**Cons**
- Modifies a shared library component — affects all DataTable instances everywhere,
  not just the KV editor. Other DataTables that also have `col_selectable(false)` and
  rely on Tab doing nothing (or expecting Tab to stay within the table) would change
  behavior.
- Tab still cycles window-globally after the fix — same end-of-table problem as
  Option A.
- Requires keeping the local gpui-component fork in sync.

**Verdict**: Quick fix that unblocks Tab, but window-global cycling remains.
Can be combined with Option B's `"KvEditor"` context to get precise control without
removing DataTable.

---

## 6. Option D — Patch DataTable + KvEditor Context (Hybrid)

Combine Options C and B:

1. Patch DataTable (`action_select_next_col` propagates when col_selectable == false).
2. Wrap the DataTable in a `div().key_context("KvEditor")` with `KvTabForward` /
   `KvTabBackward` action handlers.

The context stack becomes `["Input", "KvEditor", "DataTable", "Root"]`.
Binding list: `[IndentInline, KvTabForward, SelectNextColumn, Tab]`.

1. `IndentInline` → propagates.
2. `KvTabForward` → found on `"KvEditor"` → explicit focus → consumed.
3. `SelectNextColumn` and `Tab` never reached.

Keeps DataTable's column headers and visual structure. Gives precise Tab order.

**Pros**
- Best of both: DataTable UI preserved, Tab order is precise.

**Cons**
- Two changes: gpui-component patch + KvEditor wrapper code.
- Still heavier than Option B if DataTable is being replaced anyway.

---

## 7. Recommendation

| Option | Effort | Tab within row | Tab cross-row | Tab end wrap | DataTable kept |
|--------|--------|---------------|--------------|-------------|---------------|
| A — Plain rows | Low | ✓ | ✓ | Window-global | No |
| B — Plain rows + explicit | Medium | ✓ | ✓ | Controlled | No |
| C — Patch DataTable | Very low | ✓ | ✓ | Window-global | Yes |
| D — Patch + KvEditor ctx | Medium | ✓ | ✓ | Controlled | Yes |

**Recommended path**: **Option B**.

DataTable is being replaced anyway as part of the KV editor visual cleanup
(see `docs/ui-cleanup.md` §4.6 — borderless inputs, dynamic height, ghost add-row
button). Doing Option B at the same time as that cleanup means one pass of work
achieves both the visual and the keyboard UX goals.

If the decision is to keep DataTable, do **Option D** instead.

---

## 8. Prerequisite: Accessing InputState's FocusHandle

Option B and D both need to read a row input's `FocusHandle` from outside
`InputState`. Check if `InputState::focus_handle` is accessible:

```rust
// In gpui-component/crates/ui/src/input/state.rs
pub struct InputState {
    pub(super) focus_handle: FocusHandle,  // ← currently pub(super), not pub
    ...
}
```

If it is `pub(super)` or private, use the `Focusable` trait that `InputState`
implements — `Focusable::focus_handle(&self, cx)` returns it as a `FocusHandle`.
Since `InputState` implements `Focusable`, calling `row.key_input.focus_handle(cx)`
from the owning view should work:

```rust
let handle = row.key_input.focus_handle(cx);
window.focus(&handle, cx);
```

Verify this compiles before committing to either Option B or D.
