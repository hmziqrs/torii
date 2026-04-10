# Postman Clone Implementation Plan

> Based on `docs/state_management.md`
> Date: 2026-04-10

## 1. Goal

Build a desktop API client with Postman-like workflow parity for the core local product:

- Workspaces
- Collections
- Folders
- Environments
- Requests: REST, GraphQL, WebSocket, gRPC
- One-tab-per-item editing with one tab system that can render workspace, collection, folder, environment, or request items
- Global history and per-request history
- SQLite-backed persistence
- Drag and drop for tree items
- Local folder integration
- Git integration
- Postman-like item editing workflow and interaction model

Priority order for this plan:

1. State safety and persistence correctness
2. Request execution and tab lifecycle correctness
3. Core Postman workflow parity
4. Integrations and scale hardening
5. Visual fidelity and UX polish

## 2. Current Starting Point

The current repo is still at an early UI prototype stage:

- Basic GPUI window and sidebar scaffolding exist
- Theme, font, and radius are persisted ad hoc in `target/state.json`
- No production domain model for workspaces/collections/requests/history exists yet
- No SQLite persistence, blob storage, request engine, or tab system exists yet

That means the plan must start by replacing temporary UI-only state with the V2 state architecture before building the real Postman-style item workflow.

## 3. Locked Architecture Decisions

These are not optional if the app is meant to scale:

- Use the V2 three-tier state model:
  - Hot reactive state: active editors, visible panels, in-flight operations as `Entity<T>`
  - Warm indexed value state: normalized catalogs for workspaces, collections, folders, requests, environments, history rows
  - Cold durable state: SQLite rows plus blob files for large bodies and transcripts
- Keep `WorkspaceSession`, `TabManager`, visible request editors, and active response/session views as per-window entities
- Keep long-lived catalogs as normalized value stores keyed by IDs, not `Vec<Entity<_>>`
- Use SQLite in WAL mode for structured data and blob files for large payloads
- Use OS credential storage for secrets; do not persist raw tokens/passwords in SQLite or blobs
- Use explicit operation IDs, protocol abort handles, and terminal lifecycle states for every request/stream
- Use bounded queues, ring buffers, batching, and virtualization for large/streaming surfaces
- Treat app/window/entity update failure after async boundaries as expected behavior, not panic-worthy

## 3.1 Implementation Library Baseline

Use the existing boilerplate as the base instead of inventing a separate stack.

- `gpui` for app/window/entity/task architecture
- `gpui-component` for shared UI primitives, layouts, menus, inputs, panels, and styling conventions
- `tokio` for the async runtime used by database, networking, file watching, and background coordination
- `tokio-util` for cancellation and async coordination primitives such as `CancellationToken`
- `anyhow` for app/service-level error propagation
- `bytes` for efficient request/response body buffers and previews
- `http` for shared HTTP request/response/header/method types
- `url` for URL parsing and normalization
- `sqlx` with SQLite support for database access, migrations, and typed persistence operations
- `sea-query` as the dynamic SQL builder on top of `sqlx` for filters, ordering, tree operations, and history queries
- `reqwest` for REST and GraphQL HTTP transport
- WebSocket library:
  - prefer `tokio-tungstenite`
  - use it behind a protocol adapter so the UI/state layers do not depend on wire details
- gRPC library:
  - use `tonic` with `prost`
  - add `prost-reflect` when dynamic descriptor/reflection support is needed
  - keep reflection/descriptor handling behind a service boundary
- `keyring` for OS credential storage
- `directories` for OS-correct config/data/cache paths
- `time` for timestamps, retention windows, and human-readable time formatting
- `uuid` for stable IDs; prefer UUIDv7 for sortable persisted records
- `notify-debouncer-full` for local folder watching and debounced file change reconciliation
- `ignore` for `.gitignore`-aware local folder scanning
- Git integration library:
  - prefer a dedicated adapter layer so the app can use either `gix`, `git2`, or controlled `git` subprocesses without leaking that choice into UI/state code
- `blake3` for blob/content hashing
- `zstd` for optional response/transcript compression when persisted payload volume grows
- `serde` and `serde_json` for file-backed metadata, import/export, and structured snapshots
- `tracing` and `tracing-subscriber` for logs, metrics hooks, and failure categorization

## 3.1.1 Database Choice

Preferred choice for this app:

- `sqlx` + `sea-query`

Reasoning:

- This app is repository-driven, not ORM-driven
- A Postman-like desktop client has a lot of dynamic querying:
  - history filtering
  - tree reordering
  - descendant deletion
  - sync metadata updates
  - conflict queries
  - mixed read models for sidebars, tabs, history, and linked-folder reconciliation
- We need tight control over SQLite schema, transactions, migrations, and query shape
- `sea-query` helps with dynamic SQL construction without forcing the entire persistence model into ORM entities

Not preferred as the primary persistence layer:

- SeaORM

Why:

- SeaORM is built on top of SQLx and SeaQuery, so it adds another abstraction layer on top of the stack we would still rely on
- The core app model here is not a typical CRUD-heavy service with mostly row-to-entity mapping
- A large part of the persistence work is custom query logic, projection tables, history indexes, blob references, and sync/conflict flows where explicit SQL is usually the cleaner fit

Acceptable limited use:

- If later there is a narrow area with straightforward CRUD and clear entity relations, SeaORM could be introduced locally for that slice only
- It should not become the default persistence abstraction for the whole app unless the data model becomes much more ORM-shaped than this plan assumes

## 3.2 UI Component Rule

The current boilerplate is already built on `gpui-component`.

Rules:

- Before building a new custom component, check whether `gpui-component` already provides the needed primitive or composition path
- Prefer extending or composing `gpui-component` patterns before introducing custom low-level GPUI elements
- Only build custom components when Postman-specific behavior cannot be expressed cleanly through existing `gpui-component` building blocks
- New custom components should still follow `gpui-component` styling tokens and interaction patterns

## 3.3 Localization Rule

No raw user-facing strings should be introduced in app UI code.

Rules:

- All labels, buttons, tooltips, menus, notifications, empty states, errors, placeholders, and dialog copy must come from Fluent-based i18n
- Use the existing Fluent setup in the repo rather than ad hoc string constants
- New features are not complete until their Fluent keys and translations are added
- Raw strings are acceptable only for non-user-facing internals such as logs, metrics keys, schema names, and protocol constants

## 4. Product Model

## 4.1 Core domain objects

- `Workspace`
  - top-level container for collections, environments, history views, local/git bindings, and UI layout preferences
- `Collection`
  - named request tree owned by one workspace
- `Folder`
  - nested tree container inside a collection
- `Request`
  - leaf node with protocol-specific editor data
- `Environment`
  - variable set with workspace scope initially, expandable later to collection/request overrides
- `HistoryEntry`
  - immutable execution snapshot pointing to stored request/response metadata and blob refs
- `Tab`
  - window-local editor/view state keyed by item identity

## 4.2 Request kinds

- `RestRequest`
- `GraphqlRequest`
- `WebSocketRequest`
- `GrpcRequest`

Each kind gets:

- shared metadata: name, parent, auth, variables, labels, timestamps, revision
- protocol-specific editor model
- protocol-specific execution/session model

## 4.3 Tab rules

- One open tab per persisted item per window
- Opening an already-open item focuses the existing tab
- Unsaved new items use draft tab IDs until persisted
- Tabs are item-driven, not page-driven:
  - tab identity = item kind + item ID
  - tab renderer = the view for that item kind
  - the same tab infrastructure is reused for workspace, collection, folder, environment, and request items
- Tabs exist for:
  - workspace overview
  - collection settings/overview
  - folder overview
  - environment editor
  - REST request editor
  - GraphQL request editor
  - WebSocket session
  - gRPC request editor/session
- Deleting an item closes its tab in every window where it is open
- Deleting a parent item closes all descendant tabs across all windows
- If a deleted item has an active operation, cancel first, then close

## 4.4 Persistence split

SQLite tables should cover at minimum:

- `workspaces`
- `collections`
- `folders`
- `requests`
- `request_revisions`
- `environments`
- `environment_variables`
- `history_index`
- `history_request_refs`
- `tab_session_state`
- `ui_preferences`
- `local_folder_bindings`
- `git_bindings`
- `operation_logs`
- `schema_migrations`

Blob/file storage should cover:

- large HTTP response bodies
- WebSocket transcripts
- gRPC stream payload archives
- imported/exported collection artifacts

## 5. Phase Plan

## Phase 0 (P0): Foundation and State Contract

Goal: replace temporary prototype assumptions with the scale-ready GPUI state model from V2.

Scope:

- Introduce app-level services global:
  - database service
  - blob store
  - secret store adapter
  - filesystem watcher service
  - git service
  - metrics/logger service
- Define stable ID types and revision metadata for all domain objects
- Define normalized store interfaces for workspaces, collections, folders, requests, environments, and history rows
- Define per-window entities:
  - `WorkspaceSession`
  - `TabManager`
  - `SidebarState`
  - `WindowLayoutState`
- Move global UI preferences from ad hoc file persistence into structured settings storage
- Define operation lifecycle FSM and cancellation contract shared by all protocols
- Add performance budgets from V2 as configuration defaults

Exit criteria:

- No new feature work uses raw ad hoc global state or unbounded in-memory lists as the long-lived source of truth
- Theme/font/radius/spacing are modeled as global value state with durable storage
- Task ownership rules are documented in code: retain or detach, never accidental drop
- Basic metrics hooks exist for memory, queue depth, and cancellation outcomes

## Phase 1 (P0): SQLite, Blob Store, and Repositories [x]

Goal: make persistence crash-safe before adding real request workflows.

Detailed execution document: [docs/completed/phase-1.md](docs/completed/phase-1.md)

Scope:

- Add SQLite layer with WAL mode, migrations, and repository APIs
- Add blob storage directory and content-addressed or history-ID-based blob references
- Persist:
  - workspaces
  - collections
  - folders
  - requests
  - environments
  - UI preferences
  - history index rows
- Add secret references in DB and actual secrets in platform credential store
- Add transactional tree mutations for create, rename, move, reorder, delete
- Add crash recovery and cleanup policy for orphan blobs and partially written history rows

Exit criteria:

- App restart restores structural data without relying on ephemeral entities
- Large payload storage path exists and does not require loading full blobs into memory
- Schema migration path is test-covered
- Secret material is absent from SQLite and blob files

## Phase 2 (P0): Unified Tab System and Item Views

Goal: establish the Postman-style item editing model before protocol execution complexity expands.

Detailed execution document: [docs/phase-2.md](docs/phase-2.md)

Scope:

- Introduce one unified tab host driven by item kind and item ID
- Implement tab open/focus/close/reorder behavior
- Implement item tabs for:
  - workspace
  - collection
  - folder
  - environment
  - request
- Define the renderer contract for each tabbed item type
- Persist tab session state
- Replace current page-style views with item-driven views
- Normalize typography, spacing, border radius, and theme tokens under the new settings model

Exit criteria:

- Any supported item can be opened in exactly one tab per window
- Deleting items closes affected tabs deterministically
- Window restore can reconstruct the last tab session
- UI settings are driven by state standards, not local view-specific persistence hacks

## Phase 3 (P0): REST Request Editor and Execution Core

Goal: make the primary request workflow production-safe before adding other protocols.

Scope:

- Build REST request editor with Postman-like sections:
  - method + URL bar
  - params
  - auth
  - headers
  - body
  - scripts/tests placeholders
  - request settings
- Add request draft state in hot entities and persisted request model in repositories
- Add send/cancel/duplicate/save flows
- Add lifecycle FSM:
  - idle
  - dirty
  - sending
  - waiting
  - receiving
  - completed
  - failed
  - cancelled
- Persist response metadata and body previews with blob spillover
- Add per-request latest-run summary panel
- Add deterministic late-response ignore rules using operation IDs

Exit criteria:

- REST requests can be created, edited, sent, cancelled, duplicated, and reopened safely
- Response previews respect memory caps
- Full response bodies can be reopened from disk
- Cancelled requests never re-enter completed UI state from a late response

## Phase 4 (P1): Collections, Folders, Environments, and Drag/Drop

Goal: complete the core Postman information architecture.

Scope:

- CRUD for workspaces, collections, folders, requests, and environments
- Tree rendering with virtualization-ready design
- Tree drag/drop:
  - move request within folder/collection
  - move folder within tree
  - reorder siblings
  - move items across collections inside a workspace
- Context menus and keyboard actions matching Postman-like workflows
- Environment selector and variable resolution pipeline
- Request inheritance/resolution path for:
  - workspace variables
  - active environment variables
  - request-local overrides
- Delete semantics for parent nodes and descendant tab cleanup

Exit criteria:

- Tree mutations are transactional and persisted
- Drag/drop cannot leave the tree in an inconsistent state
- Variable resolution order is deterministic and test-covered
- Parent deletion closes descendant tabs and invalidates stale selections cleanly

## Phase 5 (P1): History and Additional Protocols

Goal: expand protocol coverage without violating V2 memory and streaming constraints.

Scope:

- Global history panel with filtering, search, grouping, and virtualization
- Per-request history panel with quick compare/reopen
- GraphQL:
  - query editor
  - variables editor
  - operation picker
  - response viewer on shared HTTP core
- WebSocket:
  - connect/disconnect
  - send message
  - bounded inbound/outbound queues
  - bounded visible ring buffer
  - transcript spill-to-disk
- gRPC:
  - unary first
  - then server streaming and bidi streaming
  - bounded decode/render queues
  - disk-backed transcript/body archive

Exit criteria:

- Global history and per-request history stay performant at large row counts
- WS and gRPC streaming do not use unbounded in-memory vectors
- Batch-notify behavior exists for stream rendering
- Protocol-specific editors reuse the shared tab, lifecycle, persistence, and history contracts

## Phase 6 (P1): Local Folder and Git Integration

Goal: make the app usable as a local-first API workspace, not only an internal DB-backed editor.

Scope:

- Local folder binding per workspace or collection
- File format for exported/linked collections, requests, environments, and metadata
- File watcher that reconciles disk changes into the warm store
- Conflict policy between DB state and filesystem state using revision and last-seen sync metadata
- Git binding on top of local folder mode:
  - repo discovery
  - branch/status display
  - changed file indicators
  - commit/pull/push/fetch affordances
  - conflict and dirty-state warnings
- Import/export flows for linked and standalone collections

Exit criteria:

- A workspace can be linked to a real folder and survive restart/reload cycles
- Disk edits can round-trip into the app without corrupting entity state
- Git status is visible and tied to linked files, not a detached side panel with no model ownership
- Sync/conflict rules are explicit and tested

## Phase 7 (P2): Clone Fidelity, Hardening, and Release Gate

Goal: close the gap between "functional API client" and "exact Postman-like desktop experience".

Scope:

- Visual parity pass:
  - tab strip behavior
  - request builder spacing
  - panel proportions
  - tree density
  - empty states
  - icons
  - selection affordances
  - keyboard shortcuts
- Split-view polish and saved layouts
- High-volume performance testing:
  - 100+ tabs
  - large collections
  - large history
  - large payloads
  - long-running streams
- Multi-window edit conflict handling
- Blob compaction and history cleanup tools
- Error states, recovery UX, and redaction audit

Exit criteria:

- Core requesting workflows feel visually and behaviorally close to Postman
- Memory plateaus under the configured caps
- Large datasets remain scrollable and interactive
- Crash, cancel, dropped-entity, and stale-window scenarios do not panic

## 6. Cross-Cutting Standards

These apply in every phase:

- No plaintext secrets in app DB, blob store, logs, exports, or crash output
- No unbounded hot-path collections for responses, history, or streams
- No protocol implementation without cancel propagation and terminal-state rules
- No large list surface without virtualization plan
- No persistent model mutation without repository transaction coverage
- No async UI update path that assumes app/window/entity survival
- No new custom UI component before checking `gpui-component` first
- No raw user-facing strings; all UI copy must be Fluent-based i18n
- No feature that bypasses metrics and structured error reporting

## 7. Required Validation Gates

Every phase should ship with the relevant tests, not as a later cleanup item.

- Unit tests:
  - ID/revision logic
  - lifecycle FSM transitions
  - variable resolution
  - drag/drop reorder rules
  - ring buffer and truncation behavior
- Integration tests:
  - restart recovery
  - send/cancel race
  - delete-item closes tab behavior across windows
  - linked-folder sync conflicts
  - git-linked workspace mutation flows
- Performance tests:
  - large collections
  - large history lists
  - 10 MB, 50 MB, and 200 MB payload handling
  - long-running WS/gRPC streams
  - multi-tab memory plateau
- Security tests:
  - secret-at-rest validation
  - export redaction
  - logging redaction
  - credential store failure handling

## 8. Recommended Execution Order Inside the Repo

1. [x] Replace current temporary settings persistence with real settings storage and services
2. [x] Land DB schema + repositories + blob storage
3. Build workspace/session/tab system
4. Build REST editor and response lifecycle
5. Build tree CRUD + drag/drop + environments
6. Add history surfaces
7. Add GraphQL, WS, and gRPC
8. Add local folder mode
9. Add Git integration
10. Run parity and hardening pass

## 9. Deliberate Scope Boundary

This plan targets local desktop workflow parity first.

Not required for initial completion unless added later:

- cloud sync
- team collaboration
- monitors
- mock servers
- public API publishing
- AI assistants/Postbot-style features

## 10. Three Recheck Passes

## Recheck 1: State Architecture Gaps

Shortcomings found in the first draft:

- UI structure work was placed too early relative to persistence and lifecycle safety
- Secret handling was not explicit enough
- Multi-window ownership and descendant tab closure rules were under-specified

Corrections applied:

- Kept Phase 0 and Phase 1 ahead of item-view fidelity work
- Added platform credential storage requirement
- Added explicit tab closure rules and multi-window-aware state split

## Recheck 2: Workflow Parity Gaps

Shortcomings found in the second draft:

- Folder, collection, and workspace tabs were not explicit enough
- Environment tabs were not explicit enough
- Environment resolution order was too vague
- Local folder and Git integration lacked conflict policy and round-trip ownership
- Cross-window delete behavior was still implicit rather than explicit

Corrections applied:

- Added explicit item-tab coverage for workspace, collection, folder, environment, and request items
- Added deterministic variable resolution expectations
- Added linked-folder sync rules, revision metadata, and Git-on-top-of-folder architecture
- Added cross-window tab close semantics for deleted items and deleted parents

## Recheck 3: Scale and Release Gaps

Shortcomings found in the third draft:

- History and streaming performance gates were not strong enough
- Blob cleanup and crash recovery were easy to defer accidentally
- "Exact Postman-like" risked becoming a visual-only task instead of a behavioral parity pass
- Validation expectations were spread across exit criteria instead of being called out as a release gate

Corrections applied:

- Added virtualization, bounded buffering, and soak/perf criteria
- Added orphan cleanup and recovery requirements
- Made Phase 7 a combined parity plus hardening gate rather than a styling-only phase
- Added an explicit validation-gates section covering unit, integration, performance, and security tests

## 11. Final Recommendation

Do not start with "clone the UI" in isolation.

The correct implementation order is:

- lock the state contract
- land SQLite/blob/secret infrastructure
- build the unified item tab system
- make REST rock-solid
- add collections/folders/environments/history
- expand protocols
- add local folder and Git
- finish with fidelity and hardening

That sequence is the fastest path to an actual Postman-like product without rebuilding the state layer later.
