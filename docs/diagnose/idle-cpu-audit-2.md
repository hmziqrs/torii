# Idle CPU Audit — Pass 2

> Date: 2026-04-16
> Scope: follow-up sweep after idle-cpu-audit-claude.md and render-loop-audit.md
> Motivation: Bugs 1-4 and RLA-1/2/4 are fixed; hunt for remaining sources

---

## Summary

Six findings in this pass are still actionable. One listed finding from the original draft (**Bug 11**) does not reproduce on the current code because `set_value()` is already guarded by the URL-change branch in `kv/sync.rs`. The most significant remaining issue is **Bug 6** (`load_html()` pushed into WKWebView on every render when HTML Preview is active) — it was flagged in the original audit's Section 7.2 but never fixed. The remaining findings are medium-to-low severity.

No timers, intervals, or recurring tasks exist. All async loops are properly bounded. The codebase is in good shape overall — the remaining issues are isolated patterns rather than systemic architecture problems.

---

## Bug 6 — `load_html()` called every render when HTML Preview tab is active 🔴 HIGH ⏳ NOT FIXED

**File:** `src/views/item_tabs/request_tab/response_panel/content_tabs.rs:204-205`

```rust
pub(super) fn render_preview_content(
    view: &mut RequestTabView,
    is_html: bool,
    html_body_for_preview: &str,
    // ...
) -> gpui::Div {
    let is_preview_active = view.active_response_tab == ResponseTab::Preview;
    if is_preview_active && is_html && !html_body_for_preview.is_empty() {
        view.ensure_html_webview(window, cx);
        if let Some(webview) = &view.html_webview {
            webview.update(cx, |w, _| {
                let _ = w.raw().load_html(html_body_for_preview);  // <-- every render
                w.show();
            });
        }
    }
    // ...
}
```

**Call chain:** `RequestTabView::render()` → `layout::render_request_tab()` → `response_panel::render_response_panel()` → `content::render_completed_response()` → `content_tabs::render_preview_content()` → `load_html()`

Every re-render of `RequestTabView` with the HTML Preview tab active pushes the full HTML string into WKWebView. Even if the content is identical, WKWebView may trigger internal layout/paint passes. No content-equality guard exists.

This was noted in render-loop-audit.md §7.2 but never assigned a fix.

**Fix:** Cache the last preview HTML on `RequestTabView` (e.g. `last_preview_html: Option<String>`). Only call `load_html` when the content actually changed.

```rust
if view.last_preview_html.as_deref() != Some(html_body_for_preview) {
    view.last_preview_html = Some(html_body_for_preview.to_string());
    webview.update(cx, |w, _| {
        let _ = w.raw().load_html(html_body_for_preview);
        w.show();
    });
}
```

---

## Bug 7 — `view.html_webview = None` drops entity during render 🟠 MEDIUM ⏳ NOT FIXED

**File:** `src/views/item_tabs/request_tab/response_panel/content_tabs.rs:210`

```rust
} else {
    view.html_webview = None;  // entity drop inside render
}
```

Dropping the WebView entity during render is a mutation inside `render()`. Per `state_management.md §4.13`, render should be a pure projection. I did not find an in-repo callback path proving this creates a render loop, so this is better classified as a render-purity / lifecycle risk than as a confirmed idle-CPU source.

**Fix:** Move the webview release to an observer or action handler (e.g. when `active_response_tab` changes away from `Preview`), rather than inside the render path.

---

## Bug 8 — `refresh_catalog` reloads catalog + notifies without equality check 🟠 MEDIUM ⏳ NOT FIXED

**File:** `src/root/tab_ops.rs:253-265`

```rust
fn refresh_catalog(&mut self, ...) {
    match load_workspace_catalog(
        &services.repos.workspace,
        &services.repos.collection,
        &services.repos.folder,
        &services.repos.request,
        &services.repos.environment,
        selected_workspace_id,
    ) {
        Ok(catalog) => self.catalog = catalog,
        Err(err) => tracing::error!("failed to refresh workspace catalog: {err}"),
    }
    cx.notify();  // always, even if catalog identical
}
```

Called after save operations. `load_workspace_catalog()` always re-queries the workspace list, and when a workspace is selected it also re-queries collections, environments, and folders/requests per collection (`src/services/workspace_tree.rs:52-104`). `cx.notify()` then fires even if the returned catalog is identical to the existing one, causing a full AppRoot re-render for no reason.

**Fix:** Compare old vs new catalog before assigning and notifying. `WorkspaceCatalog` does not currently implement `PartialEq`, so this needs either derived equality on the tree types or a dedicated change-detection helper.

```rust
let new_catalog = load_workspace_catalog(...)?;
if catalog_changed(&self.catalog, &new_catalog) {
    self.catalog = new_catalog;
    cx.notify();
}
```

---

## Bug 9 — Observer catalog reloads don't check catalog equality 🟡 LOW ⏳ NOT FIXED

**Files:** `src/root/request_pages.rs:141`, `src/root/request_pages.rs:217`

Even after the revision/identity guards (fixed in Bug 4 / RLA-2), the observers still reload the catalog and call `cx.notify()` without comparing the result to the existing catalog. If the revision changed but the catalog data didn't (e.g. internal metadata-only change), this still does a full catalog reload plus a full re-render.

**Fix:** Add the same catalog-change guard as Bug 8 at both call sites.

---

## Bug 10 — `follow_redirects_input` unguarded `cx.notify()` on invalid parse 🟡 LOW ⏳ NOT FIXED

**File:** `src/views/item_tabs/request_tab/subscriptions.rs:261-263`

```rust
let raw = state.read(cx).value().trim().to_ascii_lowercase();
let parsed = if raw.is_empty() {
    None
} else if raw == "true" || raw == "1" || raw == "yes" {
    Some(true)
} else if raw == "false" || raw == "0" || raw == "no" {
    Some(false)
} else {
    this.editor.refresh_save_status();
    cx.notify();  // fires on every invalid intermediate value
    return;
};
```

When the user types an intermediate value that doesn't match the accepted boolean spellings (for example `"t"` before completing `"true"`), `cx.notify()` fires unconditionally. This triggers a re-render for no productive purpose.

**Fix:** Remove the `cx.notify()` in the error branch, or guard it with a check that the previous value was different.

---

## Bug 11 — stale finding; current code already guards `set_value` ✅ CLEARED

**File:** `src/views/item_tabs/request_tab/kv/sync.rs:60-66`

```rust
if self.editor.draft().url != next_url {
    self.editor.draft_mut().url = next_url;
    draft_changed = true;
    self.url_input.update(cx, |s, cx| {
        s.set_value(self.editor.draft().url.clone(), window, cx);
    });
}
```

The current implementation only calls `set_value()` when `next_url` differs from the draft URL. The previously suspected unconditional `set_value()` path is not present in the current code, so this should not remain in the actionable bug list.

---

## Bug 12 — Progress consumer loop doesn't break on entity drop 🟡 LOW ⏳ NOT FIXED

**File:** `src/views/item_tabs/request_tab/request_ops.rs:192-205`

```rust
while let Some(event) = progress_rx.recv().await {
    if let Err(err) = this.update(cx, |this, cx| {
        if this.editor.active_operation_id() != Some(operation_id) {
            return;
        }
        // process event...
    }) {
        tracing::warn!(...);
        // continues looping! should break
    }
}
```

When the entity is dropped mid-execution, `this.update()` returns `Err`. The loop logs a warning but doesn't `break`, continuing to poll the channel until the execution task drops the sender. In practice the sender is scoped to the execution future, so the loop terminates eventually — but it does unnecessary work in the meantime.

**Fix:** Add `break` after the error log.

---

## Cleared — No Issues Found

These areas were audited and found to be clean:

- **Timer / interval / recurring task patterns:** None exist anywhere in `src/`.
- **Unbounded async task accumulation:** All `cx.spawn` calls are one-shot or bounded by cancellation tokens. The 7 `.detach()` calls are all event-driven observers or one-shot tasks.
- **Channel / stream patterns:** Only one `unbounded_channel` (progress events, single message). HTTP response streams are correctly bounded.
- **Background threads:** No `std::thread::spawn`. Two `tokio::task::spawn_blocking` for file I/O, both one-shot.
- **Dirty flag management:** `draft_dirty`, `response_tables_dirty`, and all four per-target KV dirty flags are properly set and cleared in bounded paths. The per-target KV flags use `std::mem::take`; `draft_dirty` and `response_tables_dirty` use direct resets.
- **`refresh(cx)` calls:** All gated behind dirty flags. No unguarded refresh calls remain.
- **`cx.observe_global` handlers:** Both in `settings.rs` have equality guards.
- **Global state mutations from input handlers:** None found.
- **Theme watcher content guard (RLA-3):** The name-equality guard at `app.rs:114` already correctly prevents cascading for same-theme file events. The remaining gap is only spurious file events for the active theme file — these are rare and low impact.

---

## Fix Priority

| # | Severity | Effort | File | Impact |
|---|----------|--------|------|--------|
| 6 | 🔴 High | Low | `content_tabs.rs:204` | Eliminates per-render WKWebView repaint when preview active |
| 7 | 🟠 Medium | Low | `content_tabs.rs:210` | Prevents entity drop mutation in render |
| 8 | 🟠 Medium | Low | `tab_ops.rs:265` | Skips unnecessary catalog reload + re-render when unchanged |
| 9 | 🟡 Low | Low | `request_pages.rs:141,217` | Same as #8 for observer paths |
| 10 | 🟡 Low | Trivial | `subscriptions.rs:261` | Removes render on invalid boolean keystrokes |
| 12 | 🟡 Low | Trivial | `request_ops.rs:192` | Prevents polling loop after entity drop |

Start with #6 (highest impact, already noted but never fixed), then #8/#9 (cheap catalog equality checks).
