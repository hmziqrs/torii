# GPUI Performance: Preventing Render Loops and Idle CPU

A pattern reference distilled from production render-loop investigations.
Each section is a named failure mode, its cause, and the canonical fix.

---

## Core Rule: render() is a pure projection

`render()` must read state and return elements — nothing else.
Any mutation inside `render()` that causes `cx.notify()` creates a feedback loop.

```rust
// BAD — entity.update() inside render schedules another render
fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    self.table.update(cx, |state, cx| {
        state.set_rows(self.compute_rows());
        state.refresh(cx); // cx.notify() → render → cx.notify() → ...
    });
    div()
}

// GOOD — only push data in when it actually changed (see dirty flags below)
fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    if self.rows_dirty {
        self.rows_dirty = false;
        self.table.update(cx, |state, cx| {
            state.set_rows(self.cached_rows.clone());
            state.refresh(cx);
        });
    }
    div()
}
```

---

## Pattern 1 — Dirty Flags for One-Time Data Push

Use a boolean flag when you need to push computed data into a child entity.
The flag is set in an event handler or constructor, cleared in render after the push.

```rust
struct MyView {
    data_dirty: bool,
    table: Entity<TableState>,
}

// In render
if self.data_dirty {
    self.data_dirty = false;
    self.table.update(cx, |t, cx| { t.set_rows(self.rows.clone()); t.refresh(cx); });
}

// In the event handler that changes data
fn on_data_received(&mut self, data: Data, cx: &mut Context<Self>) {
    self.rows = build_rows(&data);
    self.data_dirty = true;
    cx.notify();
}
```

**Key:** `std::mem::take(&mut self.data_dirty)` is a clean way to read-and-clear in one expression.

---

## Pattern 2 — Guard Every notify() Against No-Op Changes

Every `cx.notify()` schedules a full re-render. Only call it when state actually changed.

```rust
// BAD — re-renders even when value is identical
fn on_something(&mut self, new_val: String, cx: &mut Context<Self>) {
    self.value = new_val;
    cx.notify();
}

// GOOD
fn on_something(&mut self, new_val: String, cx: &mut Context<Self>) {
    if self.value != new_val {
        self.value = new_val;
        cx.notify();
    }
}
```

Same rule for `cx.observe()` callbacks that reload external data:

```rust
cx.observe(&session, |this, _, cx| {
    let new_catalog = load_catalog();
    if this.catalog != new_catalog {   // equality guard
        this.catalog = new_catalog;
        cx.notify();
    }
});
```

---

## Pattern 3 — Bidirectional Sync Must Be Event-Driven

When two UI elements stay in sync (e.g., a URL bar and a params editor), each direction
must live in its own event handler. Never centralize sync in `render()`.

```rust
// BAD — sync in render creates: render → set_value → Change → notify → render → ...
fn render(&mut self, w: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
    self.sync_inputs_from_model(w, cx); // calls set_value → emits Change → cx.notify()
    div()
}

// GOOD — each direction handled in its own subscription
fn new(cx: &mut Context<Self>) -> Self {
    // Direction 1: url_input → params editor
    cx.subscribe_in(&url_input, cx, |this, _, event, w, cx| {
        if let InputEvent::Change(val) = event {
            this.sync_params_from_url(&val, w, cx);
        }
    });
    // Direction 2: params editor → url_input (guarded)
    cx.subscribe_in(&params_editor, cx, |this, _, event, w, cx| {
        if let ParamsEvent::Changed = event {
            let new_url = this.build_url_from_params();
            if this.model.url != new_url {
                this.url_input.update(cx, |s, cx| s.set_value(new_url.clone(), w, cx));
                this.model.url = new_url;
            }
        }
    });
    // Initial population via dirty flag, not in render
    Self { draft_dirty: true, ... }
}
```

---

## Pattern 4 — Subscription Cleanup on Row Rebuild

When you rebuild a list of child entities (e.g., KV editor rows), clear the old subscriptions
first. Dropped entity handles become no-ops but the `Subscription` objects still live in memory.

```rust
struct MyView {
    rows: Vec<RowEntity>,
    row_subs: Vec<Subscription>, // separate from long-lived subscriptions
}

fn rebuild_rows(&mut self, data: &[Row], cx: &mut Context<Self>) {
    self.row_subs.clear(); // drop old Subscription objects first
    self.rows = data.iter().map(|row| {
        let input = cx.new(|cx| InputState::new(cx, row.value.clone()));
        self.row_subs.push(cx.subscribe_in(&input, cx, Self::on_row_changed));
        input
    }).collect();
}
```

Without the `clear()`, each rebuild adds 2N new subscriptions forever.

---

## Pattern 5 — Observe Precisely, Not Broadly

Observe the entity that owns the data you care about. Observing a wide entity (e.g., a parent
view) and doing expensive work (DB reads, full tree rebuilds) on every notification is the
fastest way to create per-keystroke SQLite queries.

```rust
// BAD — AppRoot observes every RequestTabView, reloads catalog on each keystroke
cx.observe(&request_tab, |this, _, cx| {
    this.catalog = load_workspace_catalog(); // 5 SQLite queries per keypress
    cx.notify();
});

// GOOD — reload catalog only when the workspace tree actually mutated
fn on_request_saved(&mut self, ...) {
    self.catalog = load_workspace_catalog();
    cx.notify();
}
// Incidental RequestTabView notifications (typing, focus) never touch the catalog
```

If you must observe broadly, gate on a revision or identity field:

```rust
cx.observe(&request_tab, |this, tab, cx| {
    let revision = tab.read(cx).revision();
    if this.last_revision == revision { return; }
    this.last_revision = revision;
    this.catalog = load_workspace_catalog();
    cx.notify();
});
```

---

## Pattern 6 — External Side Effects in Render

External calls inside `render()` — webview loads, file reads, D-Bus calls — bypass GPUI's
change detection and run every frame.

```rust
// BAD — repaints WKWebView even when HTML is identical
fn render_preview(&mut self, cx: &mut Context<Self>) -> Div {
    self.webview.update(cx, |w, _| w.load_html(&self.html)); // every render
    div()
}

// GOOD — cache and compare
fn render_preview(&mut self, cx: &mut Context<Self>) -> Div {
    if self.last_html.as_deref() != Some(&self.html) {
        self.last_html = Some(self.html.clone());
        self.webview.update(cx, |w, _| w.load_html(&self.html));
    }
    div()
}
```

Same principle applies to file watchers: debounce the callback and check whether the loaded
content actually differs from the currently applied value before notifying observers.

---

## Pattern 7 — Async Task Hygiene

When an async task holds a weak reference to an entity, break the loop on entity drop:

```rust
while let Some(event) = rx.recv().await {
    if let Err(_) = entity.update(cx, |this, cx| {
        if this.active_id != operation_id { return; } // stale operation guard
        this.process(event, cx);
    }) {
        break; // entity dropped — stop consuming the channel
    }
}
```

Also guard against processing stale results from cancelled operations by comparing an
`operation_id` rather than checking entity liveness alone.

---

## Checklist

Before shipping a new view or subscriber:

- [ ] `render()` contains no `entity.update()`, `cx.notify()`, or external I/O
- [ ] Every `cx.notify()` is inside an `if value_changed` guard
- [ ] Bidirectional sync uses event handlers, not render
- [ ] Row-rebuild helpers call `subscriptions.clear()` before creating new rows
- [ ] Observers read only from the narrowest entity that carries the changed data
- [ ] External side effects (webview, file, DB) are cached and compared before re-applying
- [ ] Async tasks `break` on entity-drop error and check `operation_id` for staleness
