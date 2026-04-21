# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Torii is a desktop API client (Postman-like) built in Rust on top of Zed's GPUI framework. The build and implementation plan is tracked in `docs/plan.md`; phase-specific execution docs live in `docs/phase-3.md` and `docs/completed/`. `docs/plan.md` Section 3 is the canonical V2 state architecture reference — consult it before making architectural decisions. `docs/gpui-architecture.md` is the GPUI-specific state design reference (ownership policy, memory budgets, entity reentrancy, cancellation model, secrets); `docs/gpui-performance.md` covers render-loop and idle-CPU failure modes — consult both before implementing any new view, entity, subscription, or streaming flow.

## Common commands

```bash
cargo run                       # launch the app (see main.rs → app::init)
cargo build                     # debug build
cargo test                      # run all tests (unit + integration)
cargo test --test <name>        # run one integration test file, e.g. `--test tab_manager_behavior`
cargo test <substring>          # run tests matching a name
cargo check                     # quick type-check, no codegen
cargo clippy                    # lint
RUST_LOG=trace cargo run  # verbose app logs (tracing env filter)
```

Integration tests live under `tests/` at the crate root and each file is its own test binary. Shared test helpers are in `tests/common/mod.rs` (notably `test_database()` which builds an isolated `AppPaths` + `Database` under `std::env::temp_dir()`).

SQLite migrations are in `migrations/` and compiled in via `sqlx::migrate!("./migrations")` at `src/infra/db/mod.rs`. Adding a migration means adding a new `NNNN_*.sql` file — `Database::connect` runs pending migrations automatically on startup.

Localization assets live in `i18n/{en,zh-CN}/torii.ftl` and are embedded via `es_fluent_manager_embedded` (see `build.rs` and `src/lib.rs`). Any user-facing string must go through `es_fluent::localize("<key>", None)` and have entries in both locale files — raw UI strings are a hard no.

## Architecture

### Three-tier state model (V2)

The codebase commits to a strict state split — do not reintroduce ad-hoc globals or `Vec<Entity<_>>` catalogs:

1. **Hot reactive state** — `Entity<T>` owned by a window: `AppRoot`, `WorkspaceSession`, `TabManager`, per-item tab views (`RequestTabView`, etc.). See `src/root.rs` and `src/session/`.
2. **Warm value state** — normalized catalogs loaded from SQLite, keyed by typed IDs. `services::workspace_tree::WorkspaceCatalog` is the primary read model that the sidebar and tab host project off.
3. **Cold durable state** — SQLite (WAL) for structured data + on-disk blob store for large payloads. See `src/infra/db/` and `src/infra/blobs/`.

Secrets never go into SQLite or blobs — they go through `SecretManager` → `SecretStoreRef` (OS keyring via `KeyringSecretStore`, with an `InMemorySecretStore` fallback for tests/headless).

### App services and bootstrap

`services::startup::bootstrap_app_services()` wires the whole backend: `AppPaths` → `Database` → blob store → all SQLite repositories → secret store/manager → recovery coordinator → session restore service. The result is stored as a `AppServicesGlobal(Arc<AppServices>)` GPUI global and read from windows via `cx.global::<AppServicesGlobal>()`. `build_app_services()` is the production path; if it fails, `fallback_app_services()` builds a degraded-but-working stack under `std::env::temp_dir()` so the window still opens.

`AppPaths::from_system()` uses the `directories` crate with `(APP_QUALIFIER, APP_ORGANIZATION, APP_NAME) = ("com", "torii", "torii")`. Tests use `AppPaths::from_test_base()` to get isolated config/data/cache/blobs dirs.

`Database` owns its own multi-threaded tokio runtime and exposes `db.block_on(fut)` — repositories call into it synchronously from GPUI code. Do not create a second runtime; reuse `db.block_on` or accept that `tokio::runtime::Handle::current()` will not be available off that runtime.

### Domain, repositories, services

- `src/domain/` — pure data types: typed IDs (`ids.rs` via the `typed_uuid_id!` macro — UUIDv7 for sortability), `RevisionMetadata`, and per-item models (`workspace`, `collection`, `folder`, `request`, `environment`, `history`, `preferences`, `secret_ref`). `item_id::ItemId` is the enum unifying all persisted item IDs.
- `src/repos/` — one file per aggregate, each exposing a `*Repository` trait and a `Sqlite*Repository` implementation behind a `type *RepoRef = Arc<dyn *Repository>`. New persistence goes here, not in views or services. Use `sea-query` for dynamic SQL; plain `sqlx::query` is fine for static statements.
- `src/services/` — orchestration that composes repos, blobs, and secrets: `workspace_tree` (loads the catalog), `session_restore` (rebuilds tabs on window open), `recovery` (startup blob/history cleanup), `ui_preferences` (typed settings store), `secret_manager`.

### Window, sessions, and the unified tab system

Each window's root is `AppRoot` (`src/root.rs`). It owns:
- a `WorkspaceSession` entity (selected workspace, sidebar selection, `TabManager`, `WindowLayoutState`)
- the `WorkspaceCatalog` value state
- persistent singleton pages (`SettingsPage`, `AboutPage`) and a `HashMap<RequestId, Entity<RequestTabView>>` cache so request tab state survives tab switches.

Tab identity is **item-driven**, not page-driven: a tab is `TabKey { item: ItemKey { kind: ItemKind, id: Option<ItemId> } }`. `ItemKind` covers `Workspace | Collection | Folder | Environment | Request | Settings | About`. Rules enforced by `TabManager`/`WorkspaceSession`:
- one tab per persisted item per window (`open_or_focus` focuses an existing tab instead of duplicating)
- deleting an item closes all descendant tabs (`WorkspaceCatalog::delete_closure` computes the close set)
- tab session state is persisted through `TabSessionRepository` and restored via `SessionRestoreService` on window open; `AppRoot::persist_session_state` is called after every mutation and on window close

The active tab's content is dispatched in `AppRoot::render_active_tab_content` to the renderers under `src/views/item_tabs/` (`workspace_tab`, `collection_tab`, `folder_tab`, `environment_tab`, `request_tab`). Shared tab-bar/empty-state chrome lives in `src/views/tab_host.rs`. When adding a new item kind, extend `ItemKind`, the catalog loader, the renderer dispatch, and the session restore resolver — the tab host itself is meant to be kind-agnostic.

### UI and theming

`gpui-component` provides the primitives (sidebar, menus, resizable panels, popups, notifications). Before building a custom component, check what `gpui-component` already exposes — introducing bespoke low-level GPUI elements is the exception, not the default. Theme/font/radius/locale are persisted through `UiPreferencesStore` (SQLite-backed via `preferences_repo`) and applied on startup in `app::init`. `themes/` is hot-reloaded at runtime via `ThemeRegistry::watch_dir`.

### GPUI implementation references

Before writing any new view, entity, subscription, or async task, check:

- `docs/gpui-architecture.md` — ownership policy, memory budgets, streaming/backpressure, cancellation model, entity reentrancy, secrets in async flows
- `docs/gpui-performance.md` — render-loop failure modes, dirty-flag patterns, subscription cleanup on row rebuild, notify guards, broad-observer antipatterns

Both docs carry acceptance checklists; run the relevant checklist before marking a view or service as complete.

## Conventions worth knowing

- All new UI strings must be Fluent keys in `i18n/en/torii.ftl` and `i18n/zh-CN/torii.ftl`; call sites use `es_fluent::localize("<key>", None)`.
- Prefer UUIDv7 for any new persisted ID so rows sort naturally by creation time.
- Async update paths must treat app/window/entity survival as fallible (weak-handle pattern in `cx.spawn` / `window.spawn`). Late responses from cancelled operations must be dropped by operation ID, not by checking liveness.
- Tree mutations (move/reorder/delete across collections, folders, requests) must be transactional in the repo layer — do not split them across multiple `block_on` calls.
- The current phase of work is Phase 3 (REST editor + execution core); check `docs/phase-3.md` before starting request-editor work, and `docs/plan.md` §5 for phase boundaries and exit criteria.
