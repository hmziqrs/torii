# Postman Clone Phase 1 Executable Plan

> Derived from `docs/POSTMAN_CLONE_IMPLEMENTATION_PLAN.md` Phase 1
> Constrained by `docs/STATE_MANAGEMENT_RESEARCH_V2.md`
> Date: 2026-04-10

## 1. Objective

Land the Phase 1 persistence backbone in a way that is immediately usable by later phases:

- SQLite in WAL mode with migrations
- Blob storage for large payloads
- Repository layer for structural data
- Secret storage split between DB references and OS credential storage
- Crash recovery and cleanup primitives
- Replacement path for the current ad hoc `target/state.json` settings persistence

This repo is still pre-Phase-0 in code terms, so this execution plan includes a small Phase-0 carry-in slice that is required to keep Phase 1 compliant with the V2 state architecture.

## 2. Non-Negotiable V2 Rules

These rules are mandatory for every task below:

- Use the three-tier model:
  - hot reactive state as `Entity<T>` only for active UI/editor state
  - warm indexed state as normalized value stores keyed by IDs
  - cold durable state as SQLite rows plus blob files
- Do not create long-lived catalogs as `Vec<Entity<_>>` or `HashMap<Id, Entity<_>>`
- Globals are for shared services and configuration, not mutable high-churn product state
- Async app/window/entity updates after await points are fallible and must never rely on `unwrap()` or `expect()`
- Task ownership must be explicit: retain long-lived tasks, detach true fire-and-forget tasks, never cancel by accidental drop
- Secrets must never be written to SQLite, blob files, logs, exports, or crash output
- Large payload handling must preserve bounded memory by default
- Any user-facing persistence or recovery errors introduced during this phase must go through Fluent i18n

## 3. Current Repo Gap Summary

The current codebase has only prototype UI state:

- `src/app.rs` persists theme settings directly to `target/state.json`
- `src/root.rs` still uses page-style navigation rather than item/session architecture
- `src/views/settings.rs` owns settings UI state locally
- There is no domain model, repository layer, migration system, blob store, or secret store adapter

That means Phase 1 cannot start as "just add SQLx." It has to first establish service boundaries that match the V2 ownership model.

## 4. Phase 1 Deliverables

Phase 1 is complete only when all of the following exist:

- `AppServices` global with shared service handles stored as cheap-to-clone values
- App data/config/cache paths derived from `directories`, not repo-local `target/`
- SQLx-backed SQLite connection bootstrap with WAL mode and startup migrations
- Initial durable schema for:
  - `workspaces`
  - `collections`
  - `folders`
  - `requests`
  - `environments`
  - `ui_preferences`
  - `history_index`
  - secret-reference metadata table
- Blob store with atomic write flow, preview support, and orphan cleanup pass
- Keychain-backed secret adapter with DB-only opaque references
- Repository traits plus SQLite implementations for Phase 1 entities
- Transactional tree mutation APIs for create, rename, move, reorder, and delete
- Recovery path for stale temp blobs and partially written history rows
- Tests covering migrations, restart recovery, tree transactions, blob safety, and secret-at-rest rules

## 5. Proposed Module Layout

This layout keeps the current app entrypoint intact while adding clear persistence boundaries:

```text
src/
  app.rs
  domain/
    mod.rs
    ids.rs
    revision.rs
    workspace.rs
    collection.rs
    folder.rs
    request.rs
    environment.rs
    history.rs
    preferences.rs
    secret_ref.rs
  infra/
    mod.rs
    paths.rs
    db/
      mod.rs
      connect.rs
      pragmas.rs
      row_types.rs
    blobs/
      mod.rs
      writer.rs
      cleanup.rs
    secrets/
      mod.rs
      keyring_store.rs
  repos/
    mod.rs
    workspace_repo.rs
    collection_repo.rs
    folder_repo.rs
    request_repo.rs
    environment_repo.rs
    history_repo.rs
    preferences_repo.rs
    secret_ref_repo.rs
  services/
    mod.rs
    app_services.rs
    startup.rs
    recovery.rs
tests/
  migration_roundtrip.rs
  startup_recovery.rs
  tree_transactions.rs
  blob_store.rs
  secret_storage.rs
migrations/
  0001_initial.sql
  0002_indexes.sql
  0003_recovery_metadata.sql
```

Notes:

- Keep repositories and services as value/service types, not GPUI entities
- Keep `WorkspaceSession` and `TabManager` out of this phase; they belong to later hot-state work
- `history_index` is included now so blob persistence and crash recovery have a durable anchor before request execution arrives

## 6. Execution Slices

## Slice 0: Phase-0 Carry-In Scaffolding

Purpose: create the minimum service/state scaffolding required before durable persistence work starts.

Tasks:

- Add baseline dependencies from the implementation plan that are required in Phase 1:
  - `tokio`
  - `tokio-util`
  - `sqlx` with SQLite + runtime features
  - `sea-query`
  - `directories`
  - `uuid`
  - `time`
  - `bytes`
  - `keyring`
  - `blake3`
- Define a stable application identity for paths and keychain namespaces now; do not reuse the temporary crate/package identity `gpui-starter`
- Introduce `AppPaths` to resolve config/data/cache locations via `directories`
- Introduce `AppServices` as a GPUI `Global` containing shared handles:
  - database pool/adapter
  - blob store
  - secret store
  - preferences repository
  - startup/recovery coordinator
- Remove direct persistence ownership from UI code; UI can depend on repository/service interfaces only

Definition of done:

- No new persistence logic writes into `target/`
- `src/app.rs` has a single service bootstrap path instead of direct file IO for settings
- The shape of service globals is established before repository code lands

## Slice 1: SQLite Bootstrap and Migration Pipeline

Purpose: establish the durable store before any domain repository logic is built.

Tasks:

- Create `migrations/` and wire SQLx migrations into startup
- Open SQLite with startup pragmas:
  - `journal_mode = WAL`
  - `foreign_keys = ON`
  - `busy_timeout`
  - `synchronous = NORMAL` or stricter if required by tests
- Build a small DB adapter layer that owns:
  - connection setup
  - migration execution
  - transaction entrypoints
  - structured error mapping
- Add startup logging around:
  - opened DB path
  - migration version applied
  - migration failure category

Definition of done:

- Fresh start creates a working DB in the app data directory
- Restart reuses the DB without manual setup
- Migration tests cover empty DB and older-schema upgrade paths

## Slice 2: Domain IDs, Revisions, and Row Models

Purpose: lock the value-model contract before repository APIs spread through the app.

Tasks:

- Define stable typed IDs for:
  - workspace
  - collection
  - folder
  - request
  - environment
  - history entry
  - secret reference
  - blob
- Use sortable UUIDs where available and keep constructors centralized
- Define shared revision metadata:
  - `created_at`
  - `updated_at`
  - `revision`
- Define normalized domain value structs for Phase 1 persisted objects
- Define separate DB row/projection types when needed instead of leaking SQL row shapes into UI/service layers

Definition of done:

- Repositories exchange domain values, not ad hoc tuples or SQL row maps
- Revision/version fields exist for every mutable persisted object needed by later conflict handling

## Slice 3: Repository Contracts and SQLite Implementations

Purpose: make structural mutations durable and testable before tabs or request execution exist.

Tasks:

- Create repository traits for:
  - workspaces
  - collections
  - folders
  - requests
  - environments
  - history index
  - UI preferences
  - secret references
- Implement SQLite-backed versions using `sqlx` plus `sea-query` where query shape is dynamic
- Define tree invariants explicitly:
  - parent must exist
  - sibling ordering is contiguous after reorder, delete, and move-out operations
  - cross-collection moves are transactional
  - parent delete removes descendants atomically
- Add transactional mutation APIs for:
  - create
  - rename
  - move
  - reorder
  - delete
- Return durable values and result types that can later feed warm normalized stores

Definition of done:

- Tree mutations cannot leave orphan rows or invalid ordering gaps
- Repository APIs are usable without any GPUI entity dependency
- All persistence writes happen inside repository/service boundaries

## Slice 4: Blob Store and Bounded Payload Contract

Purpose: make large-body persistence safe before response workflows are added.

Tasks:

- Add blob storage root under the app data directory
- Use content-addressed storage via `blake3`, or use a stable blob ID plus hash metadata
- Implement atomic write flow:
  - write to temp file
  - fsync if needed by platform policy
  - rename into final blob location
- Persist blob metadata:
  - blob ID
  - hash
  - size
  - media type when known
  - preview/truncation metadata
- Expose read APIs that support:
  - preview bytes
  - full stream/file handle
  - existence checks
- Add orphan cleanup for:
  - stale temp files
  - blobs not referenced by durable rows after crash recovery

Definition of done:

- Large payloads have a durable path that does not require full in-memory loading
- Blob writes are crash-tolerant and restart-safe
- Blob cleanup behavior is deterministic and test-covered

## Slice 5: Secret Storage Split

Purpose: ensure Phase 1 lands with the security model required by V2 rather than patching it later.

Tasks:

- Create a secret-reference table that stores only:
  - secret ref ID
  - owning object ID
  - secret kind
  - provider/namespace metadata
  - created/updated timestamps
- Implement `SecretStore` adapter backed by `keyring`
- Define lookup keys that are stable and namespaced by app + object identity
- Build repository/service helpers that:
  - write secret refs to SQLite
  - write secret values to keychain
  - delete both consistently
  - handle keychain lookup failures as normal fallible outcomes
- Add redaction rules for logs and exported debug output

Definition of done:

- DB fixtures and blob files contain no raw secret material
- Secret lookup failures are surfaced as typed errors, not panics

## Slice 6: UI Preferences Cutover Off `target/state.json`

Purpose: use a real repository-backed settings path as the first live consumer of the new persistence stack.

Tasks:

- Create `ui_preferences` repository
- Move theme, scrollbar, font, radius, and future layout tokens into structured durable settings
- Remove `target/state.json` as a source of truth for new builds
- If legacy import is reintroduced later, keep it as a one-shot compatibility path rather than an ongoing dual-write system
- Update `src/app.rs` startup to load preferences from repositories via services
- Update settings writes to flow through repositories instead of raw file writes

Definition of done:

- App settings survive restart through SQLite-backed persistence
- `target/state.json` is no longer the active durable store
- This repo has at least one real persistence flow integrated into the running app

## Slice 7: Startup Recovery and Cleanup Coordinator

Purpose: make persistence crash-safe rather than merely persistent.

Tasks:

- Add startup recovery coordinator invoked before the main window is created
- Reconcile:
  - stale temp blob files
  - history rows marked incomplete
  - orphan blob references
- Define a partial-write policy for history/blob persistence:
  - write metadata row as pending
  - finalize row only after blob commit succeeds
  - mark failed/incomplete rows for cleanup or retry
- Emit structured logs and counters for recovery outcomes

Definition of done:

- Simulated interrupted writes recover cleanly on next startup
- Recovery behavior is deterministic and idempotent

## Slice 8: Validation Gate

Purpose: prevent Phase 1 from appearing done while still violating V2 invariants.

Required tests:

- Unit tests:
  - typed ID creation and parsing
  - revision bump behavior
  - tree reorder helpers
  - blob preview/truncation logic
- Integration tests:
  - migration roundtrip
  - restart recovery after partial blob/history write
  - transactional create/move/delete flows
  - keychain failure handling
- Security tests:
  - secret-at-rest verification against SQLite fixture contents
  - blob content scan for accidental credential persistence
- Regression tests:
  - async startup path handles dropped app/window/entity targets without panic

Definition of done:

- Phase 1 exits with passing migration, repository, recovery, and security coverage
- No test depends on manual developer setup outside temp directories and mock/fake adapters where needed

## 7. Recommended PR Breakdown

Use small, reviewable slices in this order:

1. `phase1-bootstrap`
   - dependencies
   - `AppPaths`
   - `AppServices`
   - service bootstrap wiring
2. `phase1-sqlite`
   - SQLite adapter
   - migrations
   - DB startup tests
3. `phase1-domain-and-repos`
   - typed IDs
   - revision metadata
   - repository traits and SQLite implementations
4. `phase1-blob-and-secrets`
   - blob store
   - secret store adapter
   - security tests
5. `phase1-settings-migration`
   - `ui_preferences`
   - cutover off `target/state.json`
   - running app integration
6. `phase1-recovery-and-hardening`
   - startup recovery coordinator
   - orphan cleanup
   - integration/regression coverage

Do not mix tab-system or request-execution work into these PRs.

## 8. Explicit Out of Scope

The following do not belong in this phase:

- unified item tabs
- request editor UI
- request send/cancel flows
- WebSocket or gRPC session state
- virtualization work for history/tree surfaces
- multi-window tab coordination

Those depend on this persistence layer and should not be pulled forward.

## 9. Phase 1 Acceptance Checklist

- [x] SQLite opens from app-managed data paths with WAL mode enabled
- [x] SQL migrations run automatically at startup
- [x] Domain IDs and revision metadata are stable and test-covered
- [x] Repository interfaces exist for all Phase 1 persisted objects
- [x] Tree mutations are transactional and compact sibling ordering after reorder/delete/move-out
- [x] Blob writes are atomic and restart-safe
- [x] Secret values live only in OS credential storage
- [x] UI preferences no longer use `target/state.json` as source of truth
- [x] Startup recovery cleans temp/orphan/incomplete persistence artifacts
- [x] Migration, recovery, repository, and secret-at-rest tests pass

### Status (2026-04-11)

Phase 1 is complete in this repository based on the acceptance checklist above.

Greenfield note:
- Legacy backward-compat migration from `target/state.json` into SQLite was intentionally removed per project direction.
- `target/state.json` is not used as an active durable store.

## 10. Post-Phase 1 Audit Notes

The following gaps and decisions were identified during a post-implementation audit and are recorded here for Phase 2 awareness.

### Fixed in this phase

- **No `get` by ID on collection, folder, request, environment repos** — Only `workspace_repo` and `secret_ref_repo` had single-item lookup. All four repos now expose `get(id)`. Phase 2 tab renderers need these to load item data for tab titles and content without fetching full parent lists.

- **No `rename` on `WorkspaceRepository`** — The domain struct had `rename()` but the repo trait lacked it. Added `rename(id, name)` to the workspace repo.

- **Sort-order gaps after delete** — `delete()` on collections, folders, and requests left gaps in sibling `sort_order` sequences (e.g. deleting sort_order=1 from [0,1,2] left [0,2]). All three delete implementations now fetch parent context before deleting, then recompact remaining sibling `sort_order` values to a dense 0..n sequence within the same transaction.

### Known gaps — deferred to Phase 2+

- **`sea-query` declared but not yet used** — All repo queries are hand-written SQL strings. `sea-query` is the right tool for dynamic query construction (history filtering, tree operations) and will be introduced when Phase 4/5 requires it. Not a Phase 1 issue because queries are static here.

- **`block_on` in repo calls will block the GPUI main thread** — Every repo method uses `self.db.block_on(async { ... })`. Acceptable for Phase 1 (repos are only called during bootstrap). Phase 2 will call repos from UI event handlers; those callsites must either spawn background tasks or use GPUI's `cx.background_executor().spawn()` to avoid freezing the window.

- **`bytes` and `tokio-util` are declared but unused** — Intentional pre-staging. `bytes` is needed for Phase 3 response body buffers; `tokio-util` for `CancellationToken` in request cancellation. Remove them from `Cargo.toml` if you prefer minimal deps, or leave them as a signal of planned use.

- **No tracing log redaction for secret material** — Slice 5 required "redaction rules for logs." No tracing subscriber filter exists. Currently safe because no code logs secret values, but there is no structural guard. A custom `tracing::Layer` that scrubs known key patterns is needed before Phase 3 adds request/auth logging.

- **`environment_variables` stored as JSON blob, not a separate table** — `plan.md` Section 4.4 lists `environment_variables` as a table. The implementation stores them as `variables_json TEXT` in `environments`. Variables are not individually queryable in SQL. Phase 4 variable resolution will need to parse and re-serialize the JSON in the application layer. Acceptable for Phase 2; revisit when variable-level filtering or override inheritance is needed.

- **Recovery coordinator queries request blobs via raw SQL, bypassing repo layer** — `recovery.rs:60-79` runs `SELECT DISTINCT body_blob_hash FROM requests` directly on `self.db` instead of through `RequestRepository`. Functionally correct (the orphan cleanup works), but violates the "all persistence reads go through repositories" rule from Slice 3. Left as-is since recovery has a direct `Database` handle by design; document it here and fix in a future repo cleanup.

- **No regression test for dropped GPUI entity during async startup** — Slice 8 required a test for this path. The synchronous `bootstrap_app_services()` already handles failures via `fallback_app_services()`, but there is no test that exercises entity/window loss during a GPUI async task. This requires the GPUI test harness and is deferred to Phase 2 when async UI tasks are first introduced.

## 11. First Concrete Implementation Order in This Repo

When work starts, use this exact sequence:

1. Replace direct settings file access in `src/app.rs` with service bootstrap points
2. Add `AppPaths` and `AppServices`
3. Land SQLite bootstrap and migrations
4. Define IDs, revisions, and domain value models
5. Implement repositories and tree transactions
6. Add blob store and secret store
7. Move settings persistence into `ui_preferences`
8. Add startup recovery and validation coverage

That is the shortest path to a real Phase 1 foundation without violating the V2 GPUI state model.
