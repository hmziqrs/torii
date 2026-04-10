# Postman Clone Phase 2 Executable Plan

> Derived from `docs/plan.md` Phase 2
> Constrained by `docs/state_management.md`
> Date: 2026-04-11

## 1. Objective

Land the item-driven tab architecture so the app can move from page navigation to Postman-style editing:

- one unified tab host
- one open tab per persisted item per window
- item-driven rendering for workspace/collection/folder/environment/request
- deterministic open/focus/close/reorder behavior
- persisted tab session restore

Phase 2 must produce a stable shell for Phase 3 request execution without reworking state ownership later.

## 2. Non-Negotiable V2 Rules

These are mandatory for this phase:

- Keep hot UI state in per-window entities (`WorkspaceSession`, `TabManager`, selection/layout state)
- `SidebarState` and `WindowLayoutState` from the plan are intentionally folded into `WorkspaceSession` as owned fields, not separate entities — this keeps window-local state under a single ownership root
- Do not model long-lived catalogs as `Vec<Entity<_>>` or `HashMap<Id, Entity<_>>`
- Treat post-`await` app/window/entity updates as fallible; no `unwrap()`/`expect()` on those paths
- Keep task ownership explicit (retain long-lived tasks, detach fire-and-forget intentionally)
- Use Fluent for all user-facing labels, tab titles, menus, and errors
- Persist only tab/session metadata in this phase; request execution state remains Phase 3

## 3. Current Repo Starting Point

Current code is still page-driven:

- `src/root.rs` uses `active_page: Page` with `Home/Form/Settings/About`
- `src/sidebar.rs` models static page entries, not item identities
- no `WorkspaceSession` or `TabManager` entity exists
- no durable `tab_session_state` repository/table yet

Phase 2 starts by replacing page identity with item identity.

Note: the current `Page` enum includes `Settings` and `About`, which are not persisted items. These become fixed utility tab kinds (e.g. `ItemKind::Settings`, `ItemKind::About`) that participate in the tab host but have no backing repository row and are excluded from session persistence. They render their existing views through the same renderer contract as item tabs.

Draft tabs (unsaved new items with temporary IDs, per `plan.md` Section 4.3) are explicitly deferred to Phase 3. Phase 2 only opens tabs for already-persisted items and the fixed utility kinds above.

## 4. Phase 2 Deliverables

Phase 2 is complete only when all of the following exist:

- per-window `WorkspaceSession` entity owning:
  - `TabManager`
  - sidebar selection
  - window layout tokens (for tab/split/sidebar state)
- `ItemKey`/`TabKey` identity model (`kind + id`) for persisted objects
- unified tab host UI and tab strip behavior:
  - open
  - focus existing
  - close
  - reorder
- item-tab renderers for:
  - workspace
  - collection
  - folder
  - environment
  - request
- delete/close invariants:
  - deleting item closes its tab
  - deleting parent closes descendant tabs
- durable tab session persistence and restore on restart
- tests for open/focus semantics, reorder, restore, and delete-tab cleanup

## 5. Proposed Module Layout

```text
src/
  session/
    mod.rs
    item_key.rs
    tab_manager.rs
    workspace_session.rs
    window_layout.rs
  repos/
    tab_session_repo.rs
  services/
    session_restore.rs
  views/
    tab_host.rs
    item_tabs/
      mod.rs
      workspace_tab.rs
      collection_tab.rs
      folder_tab.rs
      environment_tab.rs
      request_tab.rs
tests/
  tab_manager_behavior.rs
  item_tab_open_focus.rs
  tab_session_restore.rs
  tab_close_on_delete.rs
migrations/
  0004_tab_session_state.sql
```

Notes:

- reuse existing repos/services for workspace/collection/folder/request/environment lookups
- do not mix request send/cancel lifecycle into this phase
- keep existing settings/theme persistence unchanged except where tab/session needs integration

## 6. Execution Slices

## Slice 0a: Item Identity + Session Data Model

Purpose: define the identity and state contracts without touching routing yet.

Tasks:

- introduce `ItemKind` and `ItemKey` (`kind + stable id`)
- introduce `TabState` and `TabManager` core model (window-local)
- introduce `WorkspaceSession` entity owning tab manager, sidebar selection, and window layout state as fields

Definition of done:

- `ItemKey` equality and `TabManager` open/focus/close logic are implemented and unit-tested
- `WorkspaceSession` entity compiles and can be instantiated in isolation
- no UI or routing changes yet

## Slice 0b: Session-Driven Routing Replacement

Purpose: swap the page enum for session-driven tab routing.

Tasks:

- replace `Page`-based active content routing in `src/root.rs` with `WorkspaceSession`-driven tab routing skeleton
- wire `WorkspaceSession` creation into the window lifecycle

Definition of done:

- `root` no longer relies on hard-coded page enum for active content
- opening same item key twice resolves to focus, not duplicate tab creation

## Slice 1: Durable Tab Session Schema + Repo

Purpose: make tab state survive restart.

Tasks:

- add migration `0004_tab_session_state.sql`:
  - window/session identifier
  - tab order
  - active tab key
  - pinned/dirty metadata as needed
  - timestamps/revision fields
- add `tab_session_repo` trait + SQLite implementation
- add basic read/write API for:
  - save current tab stack
  - load tab stack by window/session
  - clear on reset

Definition of done:

- repo roundtrip persists/reloads ordered tab list and active key
- no GPUI entity types leak into repo interfaces

## Slice 2: Tab Host + Behavior

Purpose: make tab interactions deterministic.

Tasks:

- implement unified tab host view and tab strip component integration
- implement `open_or_focus(item_key)`
- implement `close(tab_key)` with active-tab fallback selection
- implement reorder APIs and ordering invariants
- preserve one-tab-per-item rule within a window

Definition of done:

- tab open/focus/close/reorder works without page routing
- reordering is stable and deterministic

## Slice 3: Item Tab Renderers

Purpose: connect tab identities to real item views.

Tasks:

- define renderer contract by item kind
- wire renderers for:
  - workspace
  - collection
  - folder
  - environment
  - request
- ensure tab titles/icons come from item data + Fluent labels

Definition of done:

- each supported item kind opens in tab host and renders correct view
- no raw user-facing strings introduced in item tabs

## Slice 4: Sidebar and Selection Integration

Purpose: keep navigation and tab state coherent.

Tasks:

- replace static sidebar page actions with item-oriented open/focus actions
- wire selection state to `WorkspaceSession`
- ensure selecting in sidebar focuses existing tab when present
- ensure tab focus updates selection (when applicable)

Definition of done:

- sidebar and tabs stay in sync for item selection/focus
- no duplicate tab creation from repeated sidebar clicks

## Slice 5: Session Restore and Multi-Window Semantics

Purpose: make tab sessions durable and window-local.

Tasks:

- add `session_restore` service:
  - restore tab session before rendering main content
  - gracefully skip missing/deleted items
- scope tab stacks by window/session id
- persist tab session on mutation and on window close

Definition of done:

- restart reconstructs tab order + active tab
- window-local state remains isolated between windows

## Slice 6: Deletion Cleanup Rules

Purpose: enforce correctness when objects are removed.

Tasks:

- on request/environment/folder/collection/workspace delete:
  - close matching tab(s)
  - close descendant tabs for parent delete
- if later phases introduce active operations on deleted items, ensure cleanup hook exists for cancel-first behavior

Definition of done:

- deleting parent items cannot leave orphaned tabs
- active tab fallback is deterministic after cascade close

## Slice 7: Validation Gate

Purpose: avoid declaring Phase 2 done with unstable tab/session behavior.

Required tests:

- Unit tests (as `#[cfg(test)]` modules in source files):
  - tab key equality and one-tab-per-item dedupe (in `item_key.rs`)
  - reorder invariants and active-tab fallback rules (in `tab_manager.rs`)
  - title/icon resolution by item kind (in renderer modules)
- Integration tests (in `tests/` directory):
  - open same item twice focuses existing tab
  - close/delete parent closes descendant tabs
  - restart restores tab order and active tab
  - restore skips missing items without panic
- Regression tests:
  - dropped window/entity during async restore paths does not panic
  - sidebar selection and focused tab remain consistent after reorder and delete

Definition of done:

- all validation tests pass
- tab/session flows are stable enough to layer Phase 3 request lifecycle work on top

## 7. Explicit Out of Scope

The following are not part of Phase 2:

- REST send/cancel execution engine
- response lifecycle FSM (`sending/waiting/receiving`) implementation
- GraphQL/WebSocket/gRPC protocol execution
- history UX and filtering surfaces
- local folder sync and git workflows

## 8. Phase 2 Acceptance Checklist

- [ ] `WorkspaceSession` and `TabManager` entities exist and own window-local tab state
- [ ] Page-driven routing is replaced by item-driven tab routing
- [ ] One-tab-per-item open/focus rule is enforced
- [ ] Tab close and reorder behavior is deterministic and test-covered
- [ ] Item tabs render workspace/collection/folder/environment/request kinds
- [ ] Tab session state persists and restores from SQLite
- [ ] Deleting items closes matching tabs; parent delete closes descendants
- [ ] Sidebar selection and tab focus are kept in sync
- [ ] All new user-facing strings are Fluent-based
- [ ] Phase 2 unit/integration/regression tests pass

## 9. First Concrete Implementation Order

1. Add item/tab identity model and `WorkspaceSession` scaffolding (Slice 0a)
2. Replace page routing with session-driven tab routing (Slice 0b)
3. Add `tab_session_state` migration and repository
4. Implement tab host with open/focus/close/reorder core behavior
5. Wire item-kind renderers and sidebar integration
6. Add restore/persist flows for tab session
7. Add delete cascade tab cleanup
8. Land validation gates and stabilize
