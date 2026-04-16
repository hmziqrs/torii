# Render Loop Audit

> Date: 2026-04-16
> Scope: full-app idle CPU investigation
> Motivation: idle CPU usage exceeds Electron-based Postman

## 1. Summary

Three categories of render-loop and render-amplification bugs found. Two are **P0** — they cause continuous or per-keystroke full re-renders with synchronous database reads. Together they explain the elevated idle and interactive CPU.

| ID | Severity | Location | Pattern |
|----|----------|----------|---------|
| RLA-1 | P0 | `response_panel.rs`, `kv_editor.rs` | `TableState.refresh(cx)` in every render frame |
| RLA-2 | P0 | `request_pages.rs`, `root/mod.rs` | Observer reloads full workspace catalog on every entity notification |
| RLA-3 | P1 | `app.rs` | Theme file watcher may fire spuriously, cascading into 3 global observers |
| RLA-4 | P1 | `kv_editor.rs` | Same table-refresh-in-render as RLA-1, for every KV table |
| RLA-5 | P2 | `request_tab.rs` | Deferred ReentrancyGuard notification guarantees double-render |

---

## 2. RLA-1: TableState.refresh() Called During Every Render

### Severity: P0

### Files

- `src/views/item_tabs/request_tab/response_panel.rs:365-372,536-538`
- `src/views/item_tabs/request_tab/kv_editor.rs:164-167`

### Problem

When a request tab is open with a completed response, `render_response_panel()` is called on every render frame. Inside it, `render_completed_response()` calls `.update()` + `.refresh(cx)` on three table entities:

```rust
// response_panel.rs:365-372
view.headers_table.update(cx, |state, cx| {
    state.delegate_mut().set_rows(header_rows.clone());
    state.refresh(cx);  // <-- cx.notify() on TableState
});
view.cookies_table.update(cx, |state, cx| {
    state.delegate_mut().set_rows(cookies.clone());
    state.refresh(cx);  // <-- cx.notify() on TableState
});
view.timing_table.update(cx, |state, cx| {
    state.delegate_mut().set_rows(timing_rows);
    state.refresh(cx);  // <-- cx.notify() on TableState
});
```

`TableState::refresh()` internally calls `cx.notify()` on the `TableState` entity. If any parent observes these entities, this creates a feedback cycle:

```
render -> update table + refresh() -> TableState notifies observers
  -> RequestTabView notified -> render again -> update table + refresh() -> ...
```

This fires on **every render** with no guard. New `Vec<TimingRow>`, `Vec<ResponseHeaderRow>`, etc. are constructed from scratch each frame — no equality check to short-circuit.

The same pattern exists in `kv_editor.rs:164-167` for all KV tables (params, headers, urlencoded body, form-data text).

### Fix

Move table data updates out of the render path. Options:

1. **Dirty flag pattern** (recommended): Set a `response_dirty: bool` flag when the response changes. In render, only call `set_rows()` + `refresh()` when the flag is set, then clear it.
2. **Subscription handler**: Update table data in a subscription handler that reacts to response/exec-status changes, not in render.

```rust
// Option 1: dirty flag
if self.response_dirty {
    self.response_dirty = false;
    self.headers_table.update(cx, |state, cx| {
        state.delegate_mut().set_rows(header_rows);
        state.refresh(cx);
    });
}
```

---

## 3. RLA-2: Full Workspace Catalog Reload on Every Keystroke

### Severity: P0

### Files

- `src/root/request_pages.rs:119-132` (persisted request observer)
- `src/root/request_pages.rs:153-189` (draft request observer)
- `src/root/mod.rs:115-132` (session observer)

### Problem

`AppRoot` registers `cx.observe()` on every `RequestTabView` entity. The observer callback reloads the entire workspace catalog from SQLite and calls `cx.notify()` on `AppRoot`:

```rust
// request_pages.rs:119-132
let subscription = cx.observe(&page, move |this, _, cx| {
    let catalog = load_workspace_catalog(&services, workspace_id);
    this.catalog = catalog;
    cx.notify();  // <-- full AppRoot re-render
});
```

Since `RequestTabView` calls `cx.notify()` on virtually every user interaction (keystrokes, selection changes, body type changes, method changes), the cascade for every keystroke is:

```
InputState value changes
  → RequestTabView subscription fires
    → editor.draft_mut().field = value
    → cx.notify() on RequestTabView
      → AppRoot.observe(&page) fires
        → load_workspace_catalog()   [5 synchronous SQLite queries]
        → AppRoot.catalog = catalog
        → cx.notify() on AppRoot
          → full re-render of entire UI tree
            (sidebar, tab bar, breadcrumbs, active tab content)
```

No debouncing. No equality guard comparing old vs new catalog. Every keystroke causes **5 SQLite reads + 2 full re-renders**.

The session observer (`root/mod.rs:115-132`) has the same pattern — any session mutation triggers catalog reload + full re-render.

### Fix

1. **Do not observe RequestTabView for catalog reloads.** The catalog only needs to reload when the workspace tree structure changes (collection/folder/request create/rename/move/delete), not on every entity notification.
2. **If observing is kept, debounce.** Use a debounce window (100-300ms) so rapid changes coalesce into a single catalog reload.
3. **Add an equality guard.** Compare the new catalog with the old one before calling `cx.notify()`.

```rust
// Option 3: equality guard
let new_catalog = load_workspace_catalog(&services, workspace_id);
if this.catalog != new_catalog {
    this.catalog = new_catalog;
    cx.notify();
}
```

---

## 4. RLA-3: Theme File Watcher May Fire Spuriously

### Severity: P1

### File

- `src/app.rs:100-116`

### Problem

`ThemeRegistry::watch_dir("./themes")` monitors 21 theme JSON files. The callback applies the loaded theme via `Theme::global_mut(cx).apply_config()`, which mutates the Theme global and triggers:

1. `cx.observe_global::<Theme>` in `app.rs:140` — writes UI preferences to disk
2. `cx.observe_global::<Theme>` in `menus.rs:16` — rebuilds menu bar
3. `cx.observe_global_in::<Theme>` in `settings.rs:16` — re-renders settings page

macOS `FSEvents`/`kqueue` can generate spurious events from IDE file saves, git operations, OS indexing, or antivirus scans. No debounce or change-detection guard on the callback.

### Fix

1. **Check whether the theme actually changed** before calling `apply_config()`.
2. **Debounce the callback** to coalesce rapid file system events.
3. **Gate on `.json` file extensions** to ignore non-relevant events.

---

## 5. RLA-4: KV Editor Table Refresh in Render Path

### Severity: P1

### File

- `src/views/item_tabs/request_tab/kv_editor.rs:164-167`

### Problem

Same pattern as RLA-1. `render_kv_table()` calls `table.update(cx, |state, cx| { state.refresh(cx); })` during every render for all four KV tables (params, headers, urlencoded body, form-data text). No dirty guard.

### Fix

Same approach as RLA-1 — dirty flag or subscription-driven update.

---

## 6. RLA-5: Deferred ReentrancyGuard Double-Render

### Severity: P2

### File

- `src/views/item_tabs/request_tab.rs:1919-1921`

### Problem

`sync_inputs_from_draft()` uses a `ReentrancyGuard` to prevent immediate re-entrancy. When the guard detects deferred work, it calls `cx.notify()` at the end:

```rust
if self.input_sync_guard.leave_and_take_deferred() {
    cx.notify();  // <-- guarantees at least one extra render
}
```

Combined with the equality guards on `set_value` calls, the deferred notification is likely unnecessary — if values already match, no subscription handler fires, so no deferred work accumulates. But if any subscription does fire (e.g., a KV row change during sync), the deferred notify guarantees a second render pass.

### Fix

Audit whether the equality guards on `set_value` calls make the deferred notification redundant. If they do, remove the explicit `cx.notify()` and rely on subscriptions to self-notify only when values actually change.

---

## 7. Additional Observations

### 7.1 AppRoot entity mutation during render

**File:** `src/root/mod.rs:377`

`release_html_webview_for_tab()` calls `.update()` on a child entity during render. Currently guarded by `previous_active_tab != active_tab` so only fires on tab switch. Not a loop source, but violates the "no mutation in render" rule.

### 7.2 WebView load_html during render

**File:** `src/views/item_tabs/request_tab/response_panel.rs:589-596`

`load_html()` is called during every render when the HTML preview tab is active. This may cause continuous WKWebView repaints even when the HTML content hasn't changed.

### 7.3 Entity creation during render

**Files:** `src/root/request_pages.rs:29`, `response_panel.rs:597`

New entities (`RequestTabView`, `WebView`) are created during the render path. Both are guarded by existence checks so only fire once, but entity creation during render is a code smell.

---

## 8. Recommended Fix Order

| Step | Fix | Expected Impact |
|------|-----|-----------------|
| 1 | RLA-1 + RLA-4: Move table refreshes out of render (dirty flags) | Eliminates continuous render loop on response tabs |
| 2 | RLA-2: Stop observing RequestTabView for catalog reloads | Eliminates 5× SQLite + full re-render per keystroke |
| 3 | RLA-3: Add change guard + debounce to theme watcher | Eliminates spurious theme cascade |
| 4 | RLA-5: Audit deferred notify necessity | Reduces double-render to single-render |
| 5 | Section 7 items: Move mutations/creation out of render | Prevents future regressions |

After steps 1 and 2, idle CPU should drop to near-zero when no user interaction is occurring, and interactive CPU during typing should drop by roughly 80% (eliminating the catalog reload and table refresh overhead).
