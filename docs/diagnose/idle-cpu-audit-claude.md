# Idle CPU Audit

> Investigation: high idle CPU usage, 2026-04-15
> Symptom: idle CPU higher than Postman (Electron) — unacceptable for a native app

---

## Summary

Four confirmed bugs, ranked by impact. The worst two interact: every `RequestTabView`
render unconditionally re-parses HTTP headers and updates three table entities (#1),
and `on_kv_rows_changed` fires without a meaningful-change guard (#3), causing more
renders. Subscription accumulation (#2) compounds this over time.

---

## Bug 1 — Response panel calls `entity.update()` 3× inside `render()` 🔴 CRITICAL ✅ FIXED

**Files:** `src/views/item_tabs/request_tab/response_panel.rs:365-372, 536-539`

```rust
// This runs inside RequestTabView::render() on EVERY render, unconditionally
view.headers_table.update(cx, |state, cx| {
    state.delegate_mut().set_rows(header_rows.clone());  // Vec clone every frame
    state.refresh(cx);                                   // rebuilds col group layout every frame
});
view.cookies_table.update(cx, |state, cx| { ... });
view.timing_table.update(cx, |state, cx| { ... });
```

On every render of `RequestTabView` (regardless of why it was triggered), this code:

1. Re-parses all HTTP response headers from stored JSON
2. Re-parses all `Set-Cookie` headers
3. Rebuilds timing phase rows
4. Calls `entity.update()` on three separate table entities — a direct violation of the
   "render must be a pure projection" rule from `state_management.md §4.13`
5. Calls `state.refresh()` → `prepare_col_groups()` → `update_header_layout()` on each

This means any incidental re-render of `RequestTabView` (focus change, cursor blink in
an input, any `cx.notify()` from anywhere) causes a burst of re-parsing and entity updates.

**Root cause:** The table API requires calling `set_rows()` + `refresh()` to feed data
in. This was placed in `render_completed_response()` as a convenient single call site,
but `render_completed_response` is called from inside `render()`.

**Fix:** Cache the parsed rows as fields on `RequestTabView` and only populate the table
entities when the response data actually changes (e.g., inside `restore_completed_response`,
`restore_failed_response`, `restore_cancelled_response`). Same pattern as the `draft_dirty`
flag used for input sync.

**Implementation:**
- `response_tables_dirty: bool` field added to `RequestTabView` struct — `src/views/item_tabs/request_tab.rs:155`
- Initialized `false` in `src/views/item_tabs/request_tab/init.rs`
- Set to `true` in `complete_exec` — `src/views/item_tabs/request_tab/request_ops.rs`
- Set to `true` after history restore — `src/root/request_pages.rs:99`
- Guard in render: `src/views/item_tabs/request_tab/response_panel/content.rs:19-76` — `table.update` + `state.refresh(cx)` only runs when `response_tables_dirty` is `true`, then flag is cleared

---

## Bug 2 — Subscription leak in `make_kv_row` / `rebuild_kv_rows` 🔴 CRITICAL ✅ FIXED

**File:** `src/views/item_tabs/request_tab/state.rs:47-64, 74-96`

```rust
pub(super) fn make_kv_row(...) -> KeyValueEditorRow {
    let key_input = cx.new(|cx| { /* new InputState */ });
    let value_input = cx.new(|cx| { /* new InputState */ });

    self._subscriptions.push(cx.subscribe_in(&key_input, ...));   // pushed, never removed
    self._subscriptions.push(cx.subscribe_in(&value_input, ...)); // pushed, never removed
    // ...
}

pub(super) fn rebuild_kv_rows(...) {
    for entry in normalized {
        rows.push(self.make_kv_row(...));  // 2 new subscriptions per row
    }
    *self.kv_rows_mut(target) = rows;  // old Entity<InputState> handles dropped here
    // but self._subscriptions still holds subscriptions for the old input entities
    // → _subscriptions grows by 2×N on every rebuild and NEVER shrinks
}
```

`rebuild_kv_rows` is called from `sync_kv_rows_with_draft`, which fires every time the
URL input changes (to sync query params into the KV editor). So editing the URL repeatedly
causes `_subscriptions` to grow without bound.

The old `Entity<InputState>` handles are dropped when rows are replaced, so the old
subscriptions become no-ops. But the `Subscription` objects themselves remain in the Vec,
consuming memory and requiring GPUI to iterate over them during event dispatch.

After a session of editing, `_subscriptions` can hold hundreds or thousands of dead entries.

**Fix:** Maintain a separate `kv_subscriptions: Vec<Subscription>` (per target or unified)
that is explicitly cleared before each `rebuild_kv_rows` call. Dropping the old
`Subscription` objects lets GPUI clean them up.

**Implementation:**
- `kv_subscriptions: Vec<Subscription>` field added to `RequestTabView` — `src/views/item_tabs/request_tab.rs:147`
- `make_kv_row` pushes to `kv_subscriptions` instead of `_subscriptions` — `src/views/item_tabs/request_tab/state.rs:56, 69`
- `rebuild_kv_rows` calls `self.kv_subscriptions.clear()` before creating new rows — `src/views/item_tabs/request_tab/state.rs:101`

---

## Bug 3 — `on_kv_rows_changed` always calls `cx.notify()` 🟠 WARNING ✅ FIXED

**File:** `src/views/item_tabs/request_tab/state.rs:236`

```rust
pub(super) fn on_kv_rows_changed(&mut self, target: KvTarget, window, cx) {
    let next = self.collect_meaningful_pairs(target, cx);

    match target {
        KvTarget::Params => {
            if self.editor.draft().params != next { /* update */ }
            // url rebuild and set_value...
        }
        KvTarget::Headers => {
            if self.editor.draft().headers != next { /* update */ }
        }
        // ...
    }

    self.editor.refresh_save_status();
    cx.notify();  // ← always fires, even if nothing in the draft actually changed
}
```

This fires on every `InputEvent::Change` from any KV row input — including events triggered
by `rebuild_kv_rows` itself (which calls `set_value` on new inputs during construction,
potentially emitting `InputEvent::Change` before subscriptions are registered, but edge
cases can slip through). Crucially, it fires even when the meaningful pairs are identical
to the draft (empty trailing row management, whitespace-only entries, etc.).

Each unnecessary `cx.notify()` here schedules a re-render of `RequestTabView`, which
runs Bug #1 again: re-parsing headers and updating three table entities.

**Fix:** Only call `cx.notify()` when the draft actually changed or when `refresh_save_status`
produced a different result.

**Implementation:**
- `draft_changed` local bool tracks whether any draft field was mutated — `src/views/item_tabs/request_tab/state.rs:211`
- `cx.notify()` and `refresh_save_status()` only called when `draft_changed` is `true` — `src/views/item_tabs/request_tab/state.rs:267-270`

---

## Bug 4 — AppRoot observers call `cx.notify()` unconditionally 🟡 MINOR ✅ FIXED (promoted to P0 by RLA-2)

**Files:** `src/root/mod.rs:130`, `src/root/request_pages.rs:130, 167, 187`

The session observer and both request page observers reload the workspace catalog and
call `cx.notify()` unconditionally, even when the catalog content is identical.

```rust
// Session observer (mod.rs)
cx.observe(&session, move |this, session, cx| {
    match load_workspace_catalog(...) {
        Ok(catalog) => this.catalog = catalog,
        Err(err) => { /* log */ }
    }
    cx.notify();  // always, even if catalog is identical to before
})

// Request page observers (request_pages.rs) — same pattern
```

This causes one extra AppRoot render per user action (tab switch, sidebar click, request
save). Not a loop, but unnecessary work on every interaction.

**Fix:** Track what actually changed (workspace ID, request revision, editor identity) and only reload the catalog + call `cx.notify()` when something structural changed.

**Implementation:**
- Session observer: `last_workspace_id` guard — catalog only reloaded when `selected_workspace_id` changes — `src/root/mod.rs:117-138`; `cx.notify()` still always fires for tab/sidebar changes
- Persisted request observer: `last_revision: Option<i64>` guard — observer returns early unless `baseline().meta.revision` changed — `src/root/request_pages.rs:123-143`
- Draft request observer: `last_identity` + `last_revision` guards — catalog reload gated on identity change (draft→persisted promotion) or revision bump — `src/root/request_pages.rs:167-219`

---

## Bug 5 — Mutation inside `render()` in AppRoot 🟡 MINOR ✅ FIXED

**File:** `src/root/mod.rs:376-378`

```rust
// Inside AppRoot::render()
if self.previous_active_tab != active_tab {
    self.release_html_webview_for_tab(self.previous_active_tab, cx); // entity.update() in render
    self.previous_active_tab = active_tab;                           // field mutation in render
}
```

One-shot (only fires once per tab switch), but calling `entity.update()` inside `render()`
is an architecture violation per `state_management.md §4.13`. If `release_html_webview`
ever calls `cx.notify()` inside its closure, this becomes a loop.

**Fix:** Move the webview release to a `cx.observe(&session, ...)` callback that watches
the active tab key, or to the tab-switching action handlers in `tab_ops.rs`.

**Implementation:**
- Moved `release_html_webview_for_tab` + `previous_active_tab` tracking into the session
  observer callback — `src/root/mod.rs:138-147`
- Removed inline mutation from `render()` — `src/root/mod.rs:389-390`

---

## Interaction Between Bugs 1, 2, 3

The three critical bugs amplify each other:

```
user types in KV row
  → InputEvent::Change (on key_input or value_input)
  → on_kv_rows_changed (Bug #3: always cx.notify())
  → RequestTabView re-renders
  → render_completed_response() called (if response shown)
  → 3x entity.update() + header/cookie/timing re-parse (Bug #1)
  → if URL changed: sync_kv_rows_with_draft → rebuild_kv_rows
  → N new InputState entities created
  → 2N new subscriptions added, old ones never removed (Bug #2)
  → new InputState set_value → InputEvent::Change for each new row
  → on_kv_rows_changed fires again for each new row (Bug #3 again)
  → more cx.notify() → more renders → more table updates
```

This cascade runs on every keystroke in the KV editor when a response is visible.

---

## Fix Priority

| # | Severity | Effort | Impact |
|---|----------|--------|--------|
| 1 | 🔴 Critical | Medium | Eliminates table re-parse on every render |
| 2 | 🔴 Critical | Low | Stops subscription accumulation |
| 3 | 🟠 Warning | Low | Cuts unnecessary renders on KV input |
| 4 | 🟡 Minor | Low | One fewer render per user action |
| 5 | 🟡 Minor | Low | Architecture cleanup, future-proofs loop safety |

Start with #2 (easiest, highest leverage relative to effort), then #1, then #3.

---

## What Was NOT a Loop

For completeness, these patterns were audited and cleared:

- **Session observer** — fires only on user actions, not at idle
- **`sync_inputs_from_draft`** — correctly gated by `draft_dirty` flag
- **Input subscriptions in `subscriptions.rs`** — all have equality guards before `cx.notify()`
- **`on_kv_rows_changed` URL → params direction** — guarded: URL input subscription checks `draft.url != url` before propagating
- **Theme watcher** — uses `notify` crate with FSEvents (event-driven, not polling)
- **Settings page observers** — correctly guarded (`if this.dark_mode != dark_mode`)
- No timer-based polling loops or continuous background tasks found
