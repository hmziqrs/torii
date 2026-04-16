# Render Loop Prevention

> Investigation: request tab infinite re-render, 2026-04-15
> Scope: general GPUI pattern to avoid render-feedback loops

## 1. The Problem

Pressing up/down on any `Select` inside the request tab did nothing. Enter
and Escape worked, but directional navigation was completely broken. The
About page's Select worked fine with identical markup.

Root cause: a **render loop** was rebuilding the GPUI dispatch tree every
5-11 ms. Each rebuild tore down and recreated the Select's keyboard action
listeners before they could process `SelectUp`/`SelectDown` events.

## 2. What Caused the Loop

`RequestTabView::render()` called `sync_inputs_from_draft(window, cx)` on
every frame. That function read the draft model and called `set_value()` on
input entities to keep them in sync:

```
render()
  -> sync_inputs_from_draft()
    -> input.set_value(draft_value, window, cx)   // emits InputEvent::Change
```

`InputState::set_value` unconditionally emits `InputEvent::Change`, even
when the new value equals the old one. The request tab had subscriptions
listening for `Change` on those inputs:

```
InputEvent::Change (from set_value)
  -> subscription handler
    -> updates draft from input value
    -> cx.notify()   // schedules another render
```

This created a feedback cycle:

```
render -> sync -> set_value -> Change -> subscription -> notify -> render -> ...
```

A `ReentrancyGuard` prevented immediate re-entrancy but deferred the
`cx.notify()`, which fired on the next frame and restarted the cycle.
The loop only stopped when values happened to converge (often they
didn't, producing continuous 5-11 ms re-renders).

## 3. Why It Only Affected Certain Views

The About page had no bidirectional sync between inputs and a draft model,
so no loop was possible. The request tab had a bidirectional sync pattern:

```
inputs <--sync_inputs_from_draft-- draft
inputs --subscriptions-----------> draft
```

Running the "draft -> inputs" direction inside `render()` on every frame
was the mistake. The "inputs -> draft" direction (subscriptions) would then
trigger another render, restarting the cycle.

## 4. The Fix

### 4.1 Don't sync in render

`sync_inputs_from_draft` was removed from `render()`. It now only runs
once during construction (`draft_dirty: true`) and when explicitly
requested via `mark_draft_dirty()`.

```rust
impl Render for RequestTabView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.draft_dirty {
            self.sync_inputs_from_draft(window, cx);
            self.draft_dirty = false;
        }
        // ... rest of render
    }
}
```

### 4.2 Sync is event-driven, not render-driven

The two directions of sync are now handled in their respective event
handlers instead of on every render:

- **URL input -> params KV editor**: The URL `Change` subscription
  parses query params and calls `sync_kv_rows_with_draft(Params)`.

- **Params KV editor -> URL input**: `on_kv_rows_changed(Params)` rebuilds
  the URL with `url_with_params()` and calls `set_value()` on the URL input.

Both are loop-safe because equality guards prevent redundant updates:
if the value already matches, the handler returns early without calling
`cx.notify()`.

## 5. Guidelines

### Rule: never call `set_value` (or any state-mutating method that emits
events) from inside `render()`.

`render()` must be a pure projection of state into UI. Any mutation inside
it can trigger subscriptions that call `cx.notify()`, scheduling another
render, creating a loop.

### Rule: keep bidirectional sync event-driven.

When two UI elements need to stay in sync (e.g., URL bar and params
editor), handle each direction in its own event handler (subscription
callback). Don't centralize sync in `render()`.

### Rule: always guard against no-op mutations.

Before calling `set_value` or updating state in an event handler, compare
against the current value. This prevents the feedback loop even if both
directions fire:

```rust
// Safe: guards against no-op
if draft.url != new_url {
    draft.url = new_url;
    cx.notify();
}
```

### Rule: be aware that `set_value` always emits `Change`.

`InputState::set_value` emits `InputEvent::Change` even when the value is
identical. This means `set_value` inside `render()` is especially
dangerous — it creates a loop even when nothing actually changed.

### Rule: use `draft_dirty` flags for one-time initialization.

If you need to push model state into inputs (e.g., on tab switch or initial
load), use a boolean flag that's set when the draft changes and cleared
after syncing. This runs the sync exactly once instead of every frame.

```rust
// In the struct
draft_dirty: bool,  // set true on construction, false after first sync

// In render
if self.draft_dirty {
    self.sync_inputs_from_draft(window, cx);
    self.draft_dirty = false;
}
```
