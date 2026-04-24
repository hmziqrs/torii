# Postman Clone Phase 4 Executable Plan

> Derived from `docs/plan.md` Phase 4
> Constrained by `docs/gpui-performance.md`
> Builds on `docs/completed/phase-3.5.md`

## 1. Objective

Complete the core Postman information architecture on top of the Phase 3.5 request editor:

- collection storage types so the app does not assume every collection is SQLite-backed
- transactional CRUD for workspaces, collections, folders, requests, and environments
- a sidebar tree that is ready for virtualization rather than hard-coded recursive rendering
- drag/drop that can safely move and reorder items across the workspace tree
- an active-environment selector and a deterministic variable-resolution pipeline
- linked-collection UX affordances in the sidebar (right-side Git indicator + hover details)
- parent-delete semantics that clean up persisted tabs, draft tabs, stale selection, and active environment state

Phase 4 is the point where the app stops being "a request editor with a sidebar" and becomes a real workspace model.

Collection-type direction for this phase:

- `Managed` collection:
  - source of truth is SQLite
  - this is the future cloud-sync-capable collection type
  - cloud sync itself is explicitly out of scope for the current release
- `Linked` collection:
  - source of truth is a local folder on disk
  - this is the collection type intended to live inside a Git repository
  - full Git UX is still a later slice; the file-backed collection model is not

Important architecture rule:

- use storage-type language in the domain, not sync-product language in the persistence layer
- the code should care about `Managed` vs `Linked`, not about "cloud" vs "git" as the primary branching axis
- for `Linked` collections, treat Git operations as producers of filesystem change, not as a second source of truth

## 1.1 Rolling Implementation Updates (As-Built)

The following items were implemented on-the-fly during Phase 4 and are now the source of truth for current behavior:

- linked metadata path is `.torii/collection.json` (hard cutover from root `.collection.json`)
- linked file format uses:
  - request files: `*.request.json`
  - environment files: `*.env.json` at collection root
  - no per-folder metadata file
- linked metadata auto-initialization is enabled:
  - when opening a linked root with missing `.torii/collection.json`, metadata is created automatically
  - bootstrap reconstructs folder/order state from existing folders and `*.request.json` files before writing metadata
- no legacy compatibility mode for old linked filenames/paths in this greenfield project
- linked collection creation UI uses a native directory picker (not a plain text-only path flow)
- linked collection monitor lifecycle is gated:
  - monitor starts only when the selected workspace has at least one linked collection root
  - monitor is not started globally by default
- environment domain scope remains workspace-scoped in the current implementation (`Environment { workspace_id, ... }`)
- sidebar collection tree now renders from a flat row projection instead of recursive child rendering
- collection/folder expansion state is explicit per workspace and is persisted through `tab_session_workspace_state.expanded_items_json`

## 2. Non-Negotiable GPUI Performance Rules

`docs/gpui-performance.md` applies directly to this phase. The tree, drag/drop, and variable editors are exactly the kind of surfaces that create idle CPU regressions if they are implemented carelessly.

Mandatory rules:

- `render()` stays a pure projection:
  - no `entity.update()`
  - no `cx.notify()`
  - no `cx.subscribe()` / `cx.observe()`
  - no `cx.spawn()`
  - no repo reads, file IO, or drag/drop side effects
- Tree flattening, drop-target calculation, variable parsing, and environment resolution happen in event handlers or dedicated services, not in `render()`.
- Every `cx.notify()` stays behind a real change guard. Expansion toggles, active-environment changes, rename state, and drag-hover state must not notify when the effective value did not change.
- Subscriptions for row editors are created once per editor lifetime and cleared before any row rebuild. Rebuilding variable rows must not leak subscriptions.
- Do not observe broad entities and reload the whole workspace catalog on every keystroke. Catalog refreshes are allowed only after structural mutations or explicit repo writes.
- Hover-driven behaviors such as delayed auto-expand during drag must use explicit task ownership plus stale-target guards. No fire-and-forget spawn loops tied to render cadence.
- Any async task that updates an entity after `await` must treat entity/window/app drop as normal. Break or return quietly; never panic.
- Re-entrant mutable updates are not allowed. Tree selection, tab focus, environment selection, and request editor updates must be sequenced through events, not nested self-updates.

Phase-4-specific interpretation:

- The sidebar must render from a flat, precomputed row snapshot rather than recursively constructing a fresh widget tree with implicit expansion state every frame.
- Variable resolution runs on explicit send or explicit preview actions only. It must not recompute resolved requests on every keypress or every frame.

## 3. Current Repo Starting Point

This section is the phase-start baseline snapshot. For implemented deltas, use §1.1 as the current source of truth.

The repo already has useful low-level primitives, but the UI and state layers are still much thinner than the Phase 4 scope implies.

What already exists:

- SQLite-backed repos for:
  - `workspaces`
  - `collections`
  - `folders`
  - `requests`
  - `environments`
- Mutation primitives already exist at the repo layer:
  - `workspace_repo`: create, rename, delete
  - `collection_repo`: create, rename, move_to_workspace, reorder, delete
  - `folder_repo`: create, rename, move_to, reorder, delete
  - `request_repo`: create, rename, move_to, reorder, delete, duplicate, save
  - `environment_repo`: create, rename, update_variables, delete
- Delete closure for persisted items already exists in `src/services/workspace_tree.rs` and is used by `AppRoot::delete_item`.
- Request tabs already support unsaved draft requests tied to a `collection_id`.

What is still missing:

- at phase start, collections were implicitly SQLite-only; remaining work is broader store-boundary adoption across all tree/tab flows
- watcher/reconcile event mapping exists, but fine-grained reconcile application beyond catalog refresh is still pending
- `src/root/sidebar.rs` renders a fully materialized recursive tree using `SidebarMenuItem::children`.
- The current tree has no explicit expansion state model, no flat row model, no keyboard navigation contract, and no drag/drop support.
- Child ordering is not Postman-like yet:
  - `build_tree_items()` in `src/services/workspace_tree.rs` uses a per-kind `sort_order` sequence; the current render layer separates folders from requests rather than merging them by a unified sibling rank
  - folders appear before requests under the same parent regardless of insertion order
  - mixed sibling reordering (folder A, request B, folder C) is therefore impossible today
- UI CRUD is minimal:
  - sidebar exposes delete for most items
  - collections expose "new request"
  - requests expose duplicate
  - create/rename/move flows for the rest of the tree are absent
- `WorkspaceSession` currently stores:
  - selected workspace
  - sidebar selection
  - tab manager
  - window layout
  - it does not store:
    - active environment
    - per-workspace expansion state
    - pending drag/drop state
- `src/views/item_tabs/environment_tab.rs` is read-only and dumps `variables_json` as raw text.
- `src/views/item_tabs/workspace_tab.rs`, `collection_tab.rs`, and `folder_tab.rs` are summary cards, not management surfaces.
- Request execution still treats `{{variable}}` placeholders as literal text after logging a warning in `src/services/request_execution.rs`.
- There is no workspace-variables model and no request-local variable override model.
- Environment values are currently persisted directly in SQLite via `environments.variables_json`, which is acceptable for a demo but not acceptable for a real variable system if secrets are involved.

## 4. Critical Gaps to Close Before UI Polish

Five structural gaps need to be solved before the phase is considered executable:

1. Collection storage type boundary
   The current code assumes the collection body always lives in SQLite tables. That will force a second architecture pass later if Phase 4 does not introduce `Managed` vs `Linked` collection storage now.

2. Mixed sibling ordering
   The current tree model cannot express "folder A, request B, folder C" ordering under the same parent because folders and requests are grouped separately.

3. Session-scoped workspace state
   Active environment and expansion state are workspace-local window state, not global repo state. They need their own home in session persistence.

4. Variable storage and resolution
   A real environment selector is meaningless unless requests actually resolve variables through a deterministic precedence chain. The repo currently has nowhere to store workspace variables or request-local overrides.

5. Filesystem-authoritative reconcile path
   Linked collections need a single watcher/reconcile model for disk edits, branch checkout, merge, and pull results. If Phase 4 skips that design constraint, later Git work will grow a parallel refresh path that fights the normal collection UI state.

If these five are not addressed first, the rest of the phase will accumulate ad hoc state and render-loop risk.

## 5. Phase 4 Deliverables

Phase 4 is complete only when all of the following exist:

- collection type support for:
  - `Managed` SQLite-backed collections
  - `Linked` local-folder-backed collections intended for Git workflows
- one collection storage/provider boundary used by tree, tab, request, and environment flows
- CRUD affordances for:
  - workspace
  - collection
  - folder
  - request
  - environment
- a flat, deterministic, virtualization-ready tree row model for the sidebar
- drag/drop for:
  - collection reorder within a workspace
  - folder move/reorder within and across collections in the selected workspace
  - request move/reorder within and across folders/collections in the selected workspace
- context menus and keyboard actions for tree-oriented workflows
- workspace variables, environment variables, and request-local variable overrides
- an active environment selector scoped to the selected workspace inside a window session
- request send-path variable resolution with deterministic precedence:
  - request-local overrides
  - active environment variables
  - workspace variables
- linked-collection affordances in the sidebar:
  - right-aligned Git icon on collection rows when `storage_kind = Linked`
  - hover popover/tooltip with linked-root details and related actions
- delete semantics that:
  - close persisted descendant tabs
  - close draft request tabs whose owning collection/folder is deleted
  - clear stale selection
  - clear the active environment if it was deleted
- test coverage for tree mutation legality, variable precedence, and delete cleanup

## 6. Scope Boundary

Included in Phase 4:

- collection type model and adapter boundary for `Managed` and `Linked` collections
- linked collection file layout and local read/write round-trip
- linked collection watcher/reconcile contract and a basic filesystem watcher implementation
- the REST request editor's variable-resolution path
- structured variable editors for workspace, environment, and request scopes
- transaction-safe tree mutations
- session persistence for active environment and expansion state
- keyboard navigation and context-menu parity for the workspace tree
- linked-collection discoverability UX in the sidebar (Git badge + hover details)

Explicitly deferred:

- cloud sync for managed collections
- remote Git workflows (`fetch`, `pull`, `push`) and branch/status UX
- GraphQL, WebSocket, and gRPC variable resolution
- collection/folder duplication and import/export
- cross-workspace drag/drop for tree items
- full visual virtualization if the GPUI primitive is not ready
  - the row model must be virtualization-ready now
  - the actual virtualized renderer can remain a later swap if needed
- advanced secret-vault UX such as bulk reveal/rotate/import
- history-level resolved-variable inspection

## 7. Data and State Design

## 7.1 Collection Types and Storage Authority

Phase 4 should stop treating `Collection` as "always a SQLite row whose descendants also live in SQLite".

Recommended domain shape:

```rust
enum CollectionStorageKind {
    Managed,
    Linked,
}

struct CollectionStorageConfig {
    kind: CollectionStorageKind,
    linked_root_path: Option<PathBuf>,
}
```

Semantics:

- `Managed`
  - authoritative content lives in SQLite
  - future cloud sync may replicate from this authority later
  - current release only needs local SQLite behavior
  - `storage_config_json` is `'{}'` for managed collections; this is a valid no-op configuration, not a missing-value sentinel
- `Linked`
  - authoritative content lives in a local folder tree
  - the folder may be inside a Git repository
  - authoritative request/folder/environment/order state must live in Git-visible text files
  - app SQLite may store cache/index metadata and the local binding, but never the authoritative request/folder/environment/order state

Recommended persistence change:

- extend `collections` with:
  - `storage_kind TEXT NOT NULL DEFAULT 'managed'`
  - `storage_config_json TEXT NOT NULL DEFAULT '{}'`

Recommended architecture boundary:

- add a collection-scoped persistence/provider abstraction
- tree reads, CRUD, drag/drop, request load/save, and environment load/save must resolve the collection's storage adapter first
- keep Git operations in a separate adapter/service boundary; the collection store owns filesystem content, while the Git adapter owns repository operations

Recommended adapter split:

```rust
trait CollectionStore {
    // tree reads
    fn load_tree(&self, collection_id: CollectionId) -> Result<CollectionTreeValue>;

    // request operations
    fn load_request(&self, request_id: RequestId) -> Result<RequestItem>;
    fn save_request(&self, request: &RequestItem) -> Result<RequestItem>;
    fn create_request(&self, parent: ParentKey, name: &str) -> Result<RequestItem>;
    fn rename_request(&self, request_id: RequestId, name: &str) -> Result<()>;
    fn delete_request(&self, request_id: RequestId) -> Result<()>;

    // folder operations
    fn create_folder(&self, parent: ParentKey, name: &str) -> Result<Folder>;
    fn rename_folder(&self, folder_id: FolderId, name: &str) -> Result<()>;
    fn delete_folder(&self, folder_id: FolderId) -> Result<DeleteClosure>;

    // environment operations
    fn list_environments(&self, collection_id: CollectionId) -> Result<Vec<Environment>>;
    fn load_environment(&self, env_id: EnvironmentId) -> Result<Environment>;
    fn save_environment(&self, env: &Environment) -> Result<Environment>;
    fn create_environment(&self, collection_id: CollectionId, name: &str) -> Result<Environment>;
    fn delete_environment(&self, env_id: EnvironmentId) -> Result<()>;

    // move and reorder
    fn move_item(&self, item: TreeSiblingId, target_parent: ParentKey, position: SiblingPosition) -> Result<()>;
    fn reorder_items(&self, parent: ParentKey, ordered: &[TreeSiblingId]) -> Result<()>;
}
```

This is the minimal interface for Phase 4 CRUD and drag/drop. Extend as new operations require it — do not widen it prematurely.

Concrete implementations:

- `ManagedCollectionStore`
  - wraps the current SQLite repos
- `LinkedCollectionStore`
  - reads/writes the file-backed collection format

Rule:

- do not fork the UI by collection type
- tabs, tree rows, and request editors work against shared domain objects and shared IDs
- only the collection store changes underneath
- do not make the tree or request editor call Git helpers directly
- do not introduce hidden linked-collection state that lives only in app SQLite

Hard decision locked for linked collections:

- no repo-local SQLite database as the source of truth
- no reorder state stored only outside the repo
- if a branch switch or fresh clone cannot reproduce the collection from tracked files alone, the design is wrong

## 7.2 File-Backed Collection Format

For `Linked` collections, the file layout should mirror the request tree on disk.

As-built layout:

```text
<collection-root>/
  .torii/
    collection.json
  Auth/
    sign-in.request.json
    refresh-token.request.json
  Users/
    list-users.request.json
  local.env.json
```

Format rules:

- each request is one file
- folder nesting mirrors the sidebar tree
- collection-level metadata lives in `.torii/collection.json`
- environments are stored as root-level `*.env.json` files
- there is no per-folder metadata file in the current format

Reserved name rule:

- `.torii` is a reserved directory name inside a linked collection
- creating a folder or request with the name `.torii` must be rejected with a user-visible error
- this applies at the linked collection store level, not only in the UI

Stable identity rule:

- IDs must be stored inside metadata files, not inferred from paths
- renaming or moving a file/folder must not change the item's logical ID
- open tabs, history references, drag/drop, and future sync all depend on stable IDs

Locked metadata strategy:

- `.torii/collection.json`
  - format version
  - ordered root child IDs
  - `folders` array with stable folder IDs and parent links
  - `folder_child_orders` map for nested ordering
- `*.request.json`
  - request ID
  - request payload/editor fields
- `*.env.json`
  - environment ID
  - name
  - variable rows

Ordering rule:

- request files do not own their own `sort_order`
- folder files do not own their own `sort_order`
- ordering is owned by the parent metadata file:
  - collection root order lives in `.torii/collection.json`
  - folder child order also lives in `.torii/collection.json` (`folder_child_orders`)
- drag/drop and reorder mutate the parent metadata atomically

Bootstrap rule:

- if `.torii/collection.json` is missing when opening a linked collection root, initialize it automatically
- initialization bootstraps folder/order structure from the existing directory tree and `*.request.json` files

Compatibility stance:

- greenfield hard cutover: do not read or write legacy linked metadata/file names (`.collection.json`, `.torii-folder.json`, `*.torii-request.json`, `*.torii-env.json`)

Why this strategy:

- one place owns sibling order
- reorder diffs stay small and readable
- merges are easier than rewriting many per-item sort fields
- rename/move does not require changing logical IDs

Explicit rejection:

- do not use a linked-collection SQLite file for ordering/identity/state
- do not store sort order only on each request/folder file if parent order has to be reconstructed heuristically
- do not derive order from filesystem listing order

Why not path-derived IDs:

- path-derived identity breaks on rename and move
- it would force tab remap and history remap logic everywhere
- it makes future sync/conflict handling much harder than storing stable IDs up front

## 7.3 Linked Collection Cache and Rebuild Rules

Linked collections may still use app-local acceleration state, but only as disposable derived data.

Allowed cache/index examples:

- parsed tree cache
- search index
- last-seen hash/mtime index
- watcher bookkeeping
- local performance snapshots

Rules:

- app SQLite cache for linked collections is optional and disposable
- deleting the cache must not lose linked collection data
- on startup, cache miss, or cache corruption, the collection must rebuild fully from repo files
- branch switch correctness must never depend on cached SQLite state

Deterministic rebuild rule:

- the linked collection store must be able to reconstruct:
  - collection metadata
  - full tree shape
  - ordering
  - requests
  - environments
from tracked files alone

## 7.4 Linked Collection Write Semantics

Linked collection writes need transactional behavior without using SQLite as authority.

Required write rules:

- writes are staged in memory first
- each changed file is written to a temp path
- commit step is atomic replace/rename into the final path
- parent metadata is written in the same logical transaction as child create/move/delete operations
- watcher-originated self-events must be recognized so the UI does not double-apply the same mutation

Failure rule:

- if a multi-file linked mutation fails mid-write, recovery must prefer one of:
  - retry/rollback before notifying success
  - full rescan from disk and surface a recoverable error
- never leave hot state assuming success if the repo files do not reflect that success

## 7.5 Linked Collection Reconcile Model

Linked collections should follow one refresh model for all disk-originated change, including Git activity.

Recommended rule:

- disk edits, file renames, branch checkout, merge results, pull results, and external tool changes all enter the app through the same watcher/reconcile path

Recommended architecture:

```rust
struct LinkedCollectionEvent {
    collection_id: CollectionId,
    kind: LinkedCollectionEventKind,
    path: PathBuf,
}
```

Sources for `LinkedCollectionEventKind` should include:

- file added
- file changed
- file removed
- directory removed
- full rescan requested

Debounce and coalescing rule:

- the reconcile processor must coalesce filesystem events within a short debounce window (50–100 ms) before acting on them
- when the event count for a single collection exceeds a threshold (for example more than 50 events in one window), collapse the batch to a single `FullRescanRequested` event rather than processing events individually
- this prevents a Git checkout or merge that touches many files from firing hundreds of individual UI recomputes

Important constraint:

- Git actions should not trigger a bespoke `reload_after_checkout()` path in the UI
- the Git adapter performs the repository operation
- the watcher/reconcile layer observes the resulting filesystem change and updates warm/hot state

Why:

- it keeps one correctness path for normal edits and Git-driven edits
- it matches Bruno's effective model for branch switches
- it avoids duplicated stale-tab and stale-selection handling

## 7.6 Unified Tree Ordering

The current `sort_order` columns on `folders` and `requests` can be reused, but the interpretation must change.

New rule:

- `sort_order` is a unified sibling rank within a single parent container, regardless of item kind.

Implications:

- a collection root can contain folders and requests interleaved by `sort_order`
- a folder can contain folders and requests interleaved by `sort_order`
- tree rendering must merge folder and request children by shared order instead of concatenating `folders + requests`

Implementation rule:

- do not try to preserve the current "folders first, requests second" behavior once Phase 4 lands
- all create/move/reorder/delete mutations must renumber the combined sibling list transactionally

Recommended internal type:

```rust
enum TreeSiblingId {
    Folder(FolderId),
    Request(RequestId),
}
```

This becomes the mutation service's unit of ordering rather than separate per-kind reorder lists.

Compatibility rule for existing data:

- old rows can keep their current `sort_order` values
- however, the existing per-kind sequences are independent (a folder and a request under the same parent can both have `sort_order = 0`)
- the normalization pass must resolve these collisions explicitly: within each tied group, place folders before requests as the initial stable baseline, then assign unique contiguous values
- the first Phase 4 structural write under a parent must normalize the combined sibling order for that parent
- optionally run a startup normalization pass for demo data so drag/drop starts from a stable baseline without requiring the first write to trigger normalization

## 7.7 Session-Scoped Workspace State

`WorkspaceSession` needs per-workspace UI state instead of a single global selection-only model.

Recommended expandable ID type:

```rust
enum ExpandableId {
    Collection(CollectionId),
    Folder(FolderId),
}
```

Only containers can be expanded; using a purpose-built type prevents non-sensical entries such as request IDs or `None`-id draft keys in the expansion set.

Recommended hot state:

```rust
struct WorkspaceScopeState {
    expanded_items: BTreeSet<ExpandableId>,
    active_environment_id: Option<EnvironmentId>,
}
```

Recommended ownership:

- `WorkspaceSession` keeps:
  - `selected_workspace_id`
  - `sidebar_selection`
  - `tab_manager`
  - `window_layout`
  - `workspace_scopes: BTreeMap<WorkspaceId, WorkspaceScopeState>`

Persistence recommendation:

- add a dedicated session table rather than overloading `tab_session_metadata` with JSON blobs:

```text
tab_session_workspace_state (
  session_id,
  workspace_id,
  active_environment_id,
  expanded_items_json,
  created_at,
  updated_at,
  revision
)
```

Why a separate table:

- active environment is workspace-scoped, not session-global
- expansion state can grow independently
- this avoids turning `tab_session_metadata` into an opaque catch-all JSON row

Persistence rules:

- only durable workspace UI state is persisted:
  - active environment
  - expanded item keys
- ephemeral drag-hover state, rename mode, and dialog state are never persisted

## 7.8 Flat Tree Read Model

The sidebar should stop rendering directly from nested `CollectionTree` / `FolderTree` recursion.

Recommended read model:

```rust
struct FlatTreeRow {
    row_key: SharedString,
    item: ItemKey,
    depth: u16,
    is_container: bool,
    is_expanded: bool,
    has_children: bool,
    request_method: Option<String>,
}
```

Rules:

- flatten the selected workspace tree into `Vec<FlatTreeRow>` using the current `WorkspaceScopeState`
- recompute rows only when one of these changes:
  - selected workspace
  - workspace catalog
  - expansion state for that workspace
- `render_sidebar()` consumes the already-flattened rows and produces UI
- `render_sidebar()` does not recurse into child tree structures

This is the key step that makes virtualization possible later without rewriting selection, drag/drop, or keyboard logic.

## 7.9 Variable Model and Persistence

Phase 4 needs one shared variable shape across three scopes:

- workspace variables
- environment variables
- request-local overrides

Recommended domain model:

```rust
enum VariableValue {
    Plain { value: String },
    Secret { secret_ref: Option<String> },
}

struct VariableEntry {
    key: String,
    enabled: bool,
    value: VariableValue,
}
```

Persistence recommendation:

- add `workspaces.variables_json TEXT NOT NULL DEFAULT '[]'`
- add `requests.variable_overrides_json TEXT NOT NULL DEFAULT '[]'`
- keep `environments.variables_json`, but move it to the same row-array shape

Storage rule by collection type:

- for `Managed` collections, request variables and environment variables are persisted via SQLite-backed repos
- for `Linked` collections, request and environment artifacts are persisted in the linked collection root (`*.request.json`, `*.env.json`) with ordering metadata in `.torii/collection.json`
- workspace variables remain workspace-scoped SQLite data unless the product later introduces linked workspaces

Environment scope (current implementation):

- environments remain workspace-scoped in the domain model (`Environment { workspace_id, ... }`)
- users can create/select environments without requiring a specific collection selection first
- linked collections can still persist environment artifacts to disk while environment selection and session state stay workspace-scoped

Important security rule:

- plain variables may persist inline in SQLite
- secret variables persist only `secret_ref`
- secret values themselves stay in the existing secret store

Legacy compatibility:

- environment repo readers should accept both:
  - current object-map JSON (`{"baseUrl":"..."}`)
  - new row-array JSON (`[{"key":"baseUrl","enabled":true,"value":{"Plain":{"value":"..."}}}]`)
- an empty object `{}` is treated as zero variable entries
- when converting from object-map to row-array, preserve the natural iteration order of the JSON object rather than sorting alphabetically; this keeps the initial display order predictable for existing users
- next successful save of an environment rewrites the value into the row-array format

This avoids brittle SQL JSON migration logic and keeps seeded demo data readable.

## 7.10 Resolution Semantics

Resolution precedence is locked:

1. request-local overrides
2. active environment variables
3. workspace variables

Resolution rules:

- only enabled entries with non-empty keys participate
- duplicate keys within the same scope are resolved by last enabled row wins
- no resolved value is written back into the request draft
- resolution happens on explicit send, and optionally on an explicit preview/debug action
- resolution does not run continuously while typing

`ResolvedRequest` type rule:

- introduce a distinct `ResolvedRequest` type that is separate from `RequestItem`
- `ResolvedRequest` has no persistence path; no repo accepts it as a save target
- this prevents accidental `request_repo.save(resolved_request)` at the type level
- resolution produces a `ResolvedRequest`; execution consumes it

Fields that should resolve in Phase 4:

- URL
- query param keys and values
- header keys and values
- auth username and API-key name
- raw text / raw JSON body content
- URL-encoded keys and values
- form-data text field keys and values

Fields that should not resolve in Phase 4:

- keychain secret payloads returned from `SecretStore`
- binary body blob contents
- multipart file bytes

Missing-variable rule:

- unresolved placeholders become a preflight validation failure
- the request is not sent
- the UI must surface the missing variable names and the scope chain that was checked
- the failure is displayed inline below the URL bar in the request tab, as a sticky notice that replaces the normal "ready to send" state; it clears as soon as the variable is defined or the send is retried successfully

Observability rule:

- add tracing for `variable.resolve`
- log counts and names of missing variables, but do not log resolved secret values

## 8. Proposed Module Layout

```text
src/
  domain/
    collection.rs          ← extend with CollectionStorageKind, CollectionStorageConfig
    variable.rs            ← new: VariableEntry, VariableValue, ResolvedRequest
  repos/
    workspace_repo.rs
    environment_repo.rs
    request_repo.rs
    tab_session_repo.rs
  services/
    collection_store.rs
    workspace_tree.rs
    tree_mutation.rs
    variable_resolution.rs
    linked_collection_reconcile.rs
  infra/
    linked_collection_format.rs
  session/
    workspace_session.rs
  root/
    sidebar.rs
  views/
    item_tabs/
      workspace_tab.rs
      collection_tab.rs
      folder_tab.rs
      environment_tab.rs
      request_tab/
        variables_editor.rs
tests/
  linked_collection_roundtrip.rs
  linked_collection_reconcile.rs
  sidebar_tree_flatten.rs
  tree_mutation_drag_drop.rs
  variable_resolution.rs
  delete_cleanup.rs
  session_workspace_state.rs
migrations/
  0003_phase4_tree_and_variables.sql
```

Notes:

- `CollectionStorageKind` and `CollectionStorageConfig` extend the existing `src/domain/collection.rs` rather than living in a new file — they are attributes of the `Collection` domain type, not a separate module
- `variable.rs` is new and also owns `ResolvedRequest`; do not put `ResolvedRequest` inside `request.rs` where it could be confused with a persistable type
- `git_service.rs` is deliberately absent; remote Git workflows are out of scope for Phase 4 and belong in a later phase module
- keep using `src/services/workspace_tree.rs` as the primary read-side module, but extend it to produce flat rows and unified ordering
- add a new mutation service rather than trying to force cross-kind drag/drop semantics into the existing per-repo reorder APIs
- reuse the existing request KV editor patterns where they fit, but do not shoehorn variable secrets into `KeyValuePair`

## 9. Execution Slices

### Current Status

- Slice 0 — `Done` (with workspace-scoped environments restored by `0004_workspace_scoped_environments.sql`)
- Slice 1 — `Done`
  - done: store boundary, linked format, `.torii/collection.json`, monitor wiring, native directory picker, degraded/offline linked-root UX state
- Slice 2 — `Partially Done` (flat row model + persisted expansion landed; expansion keyboard bindings pending)
- Slice 3 — `Partially Done`
  - done: core create flows (workspace/collection/environment/folder/request), linked Git badge tooltip, folder-level request creation
  - pending: full rename/edit parity and richer linked badge actions
- Slice 4 — `Pending` (tree drag/drop mutation engine)
- Slice 5 — `Done` (active environment selection + session persistence + clear-on-delete)
- Slice 6 — `Mostly Done`
  - done: variable resolution pipeline and send-path integration
  - pending: full missing-variable preflight UX parity for every field/scope case
- Slice 7 — `Partially Done`
  - done: delete cleanup for active environment/session state and persisted closures
  - pending: full draft-descendant closure parity in all delete paths
- Slice 8 — `Pending`

## Slice 0: Persistence and Domain Contracts

Purpose: add the minimum state model required for the rest of the phase.

Tasks:

- add migration `0003_phase4_tree_and_variables.sql`
  - `ALTER TABLE collections ADD COLUMN storage_kind TEXT NOT NULL DEFAULT 'managed'`
  - `ALTER TABLE collections ADD COLUMN storage_config_json TEXT NOT NULL DEFAULT '{}'`
  - `ALTER TABLE workspaces ADD COLUMN variables_json TEXT NOT NULL DEFAULT '[]'`
  - `ALTER TABLE requests ADD COLUMN variable_overrides_json TEXT NOT NULL DEFAULT '[]'`
  - keep environments workspace-scoped (`workspace_id`) and add any needed indexes/backfill for active-environment lookups
  - create `tab_session_workspace_state`
- introduce `CollectionStorageKind` and collection storage config types in `src/domain/collection.rs`
- introduce `VariableEntry` / `VariableValue` / `ResolvedRequest` domain types in `src/domain/variable.rs`
- keep `Environment` domain type workspace-scoped (`Environment { workspace_id, ... }`)
- update repo mappings:
  - collection repo reads/writes storage kind + storage config
  - workspace repo reads/writes `variables_json`
  - environment repo queries by `workspace_id`; reads/writes row-array variables and accepts legacy map JSON
  - request repo reads/writes `variable_overrides_json`
- update `RequestItem` and request editor dirty detection to include variable overrides; unsaved-draft warnings and tab-close prompts depend on this being correct from the start
- define the linked-collection reconcile event contract (`LinkedCollectionEvent`, `LinkedCollectionEventKind`) that later watcher and Git flows will feed into

Definition of done:

- collection rows can express `Managed` and `Linked` storage
- all three variable scopes have a stable persisted shape
- `RequestItem` dirty detection covers `variable_overrides_json`
- session workspace state roundtrips independently of tab stack state
- old environment JSON still loads
- later watcher/Git work has a single reconcile contract to target

## Slice 1: Collection Store Boundary, Linked Format, and Watcher Contract

Purpose: land the storage abstraction and a working filesystem watcher before more UI code assumes SQLite-only collections.

Tasks:

- add `CollectionStore` resolution by collection ID
- implement `ManagedCollectionStore` over the existing SQLite repos
- implement `LinkedCollectionStore` over the file-backed collection format
- add create/open flows for:
  - managed collection
  - linked collection with chosen root path via native directory picker (with text path input still available)
- define the on-disk layout writer/reader for collection, folder, request, and environment items
- keep stable IDs in file metadata and never derive identity from paths
- lock parent-owned ordering metadata:
  - `.torii/collection.json` owns root child order
  - `.torii/collection.json` also owns folder child order (`folder_child_orders`)
- enforce the `.torii` reserved name rule at the `LinkedCollectionStore` level
- resolve collection-store calls without leaking Git concepts into the tree/request UI
- allow app-local cache/index data, but make full rebuild from tracked files the correctness path
- wire a basic filesystem watcher (using the `notify` crate or equivalent) that converts raw OS events into `LinkedCollectionEvent` values and routes them through the reconcile processor defined in Slice 0; implement the debounce/coalesce window from §7.5
- implement the degraded collection state: if a linked collection's `linked_root_path` is inaccessible at startup (drive removed, directory deleted, path on a network share), the collection must open in a degraded/offline state with a clear user-visible indicator, rather than crashing or silently showing an empty tree

Definition of done:

- tree and item-loading code can resolve the correct store from collection type
- adapter dispatch is correct: calling code resolves `ManagedCollectionStore` for a managed collection ID and `LinkedCollectionStore` for a linked collection ID, with an integration test covering both paths
- linked collections can round-trip locally without Git UX
- linked collection order/identity are fully reproducible from tracked text files alone
- UI/state code no longer assumes all collection descendants live in SQLite
- a linked collection with an inaccessible root path opens in a degraded state with a visible error, not a crash or empty tree
- basic watcher fires reconcile events for file add, change, and removal under a linked collection root

## Slice 2: Flat Tree Model, Session Expansion State, and Expansion Keyboard Bindings

Purpose: remove recursive tree rendering as the source of truth, and land the full expansion event surface at the same time.

Tasks:

- extend `WorkspaceSession` with per-workspace scope state using `BTreeSet<ExpandableId>`
- add expansion toggle events for collections and folders
- add keyboard bindings for tree expansion co-located with the expansion model:
  - `Left` collapses the focused container or moves focus to its parent
  - `Right` expands the focused container
- change `workspace_tree.rs` to produce flat rows from the selected workspace tree
- merge folders and requests by unified sibling order
- update `root/sidebar.rs` to render from flat rows instead of recursive `children(...)`

Performance rules for this slice:

- no `cx.observe()` inside row rendering
- no ad hoc recursive widget creation that also mutates expansion state
- no catalog reloads when only expansion state changes

Definition of done:

- the sidebar renders from a stable row vector
- expansion state is explicit, session-scoped, and persisted
- the row model is independent from the rendering widget choice
- Left/Right keyboard bindings are wired and functional

## Slice 3: CRUD Surfaces and Structured Editors

Purpose: expose the missing create/rename/edit workflows before drag/drop starts moving data around.

Prerequisites: Slice 1 (`CollectionStore` bound available).

Tasks:

- add create and rename flows for every item kind
- when creating a collection, require choosing the collection type:
  - `Managed`
  - `Linked` (root path via text input; see Slice 1 note on file picker)
- in the sidebar collection row UI:
  - keep the existing collection icon on the left
  - add a right-aligned Git icon for linked collections only
  - on hover/focus of the Git icon, show a popover/tooltip with:
    - storage type (`Linked`)
    - linked root path
    - quick actions (`copy path`, `open in finder` if supported, `manage local environments`)
- add delete confirmations where destructive behavior is ambiguous
- upgrade item tabs:
  - workspace tab:
    - rename workspace
    - create collection
    - create environment
    - edit workspace variables
  - collection tab:
    - rename collection
    - create child request
    - create child folder
  - folder tab:
    - rename folder
    - create subfolder
    - create request
  - environment tab:
    - rename environment
    - structured variable editor
- add request-local variables editor in the request tab
  - use a new `Variables` section rather than overloading `Settings`

Definition of done:

- every Phase 4 item can be created and renamed from the UI
- collection creation exposes the storage-type choice clearly
- linked collections are visually distinguishable in the tree via a right-side Git icon
- hover/focus on the Git icon reveals linked-collection metadata/actions without causing row reflow
- variable editing is structured, not raw JSON
- all new copy goes through Fluent in `i18n/en/torii.ftl` and `i18n/zh-CN/torii.ftl`

## Slice 4: Tree Drag/Drop Mutation Engine

Purpose: make tree movement safe before polishing UX.

Tasks:

- introduce drag payloads and drop intents:

```rust
enum TreeDropIntent {
    Before(ItemKey),
    Into(ItemKey),
    After(ItemKey),
}
```

- introduce legality checks:
  - no dropping a folder into itself
  - no dropping a folder into its descendant
  - no dropping onto a request as a container
  - no cross-workspace drag in this phase
  - environment rows are not drag targets
  - no cross-storage drag in this phase
    - moving items between two managed collections is allowed
    - moving items between two linked collections is allowed only if the linked-store implementation can do it transactionally
    - moving between managed and linked collections is deferred until an explicit import/export flow exists
  - no Git-aware drop path in this phase
    - drag/drop mutates the collection store only
    - any later Git status changes are observed through the reconcile path
- add `TreeMutationService` that owns transactional combined-order updates
- support:
  - collection reorder in workspace
  - folder move/reorder across collections and parents
  - request move/reorder across collections and parents
- renumber combined sibling order after each structural mutation
- drop indicator visual:
  - `Before` and `After` intents render as a horizontal line between rows at the correct index position
  - `Into` intent renders as a highlight on the target container row
  - hit-zone calculation is index-based on the flat row vector, not tree-structure-based; use row index and a configurable threshold (e.g., top 25% / bottom 25% of row height) to distinguish Before/After from Into
- optionally auto-expand a hovered closed container after a delay
  - store the task handle explicitly
  - cancel/replace it when the hover target changes

Definition of done:

- illegal drops are rejected before repo writes
- legal drops persist atomically
- tree order remains consistent after restart

## Slice 5: Active Environment Selector

Purpose: give the variable system an actual runtime selector.

Tasks:

- add an active-environment selector in the window-level chrome for the selected workspace
- populate it from workspace-scoped environments for the selected workspace (including linked-backed artifacts resolved through collection-store paths where applicable)
- persist selection in `WorkspaceScopeState`
- if the active environment is deleted:
  - clear the active environment
  - notify open request tabs only as needed for visible badges/state
- if the selected workspace changes:
  - restore that workspace's last active environment from session state

Performance rule:

- changing the active environment must not reload the entire catalog unless the environment list itself changed

Definition of done:

- active environment is a first-class part of session state
- request execution can resolve against it deterministically

## Slice 6: Variable Resolution in Request Execution

Purpose: turn the variable model into real request behavior.

Tasks:

- add `VariableResolutionService`
- before URL parsing and auth/body materialization, produce a transient `ResolvedRequest` (the distinct non-persistable type from `src/domain/variable.rs`)
- apply resolution to the Phase 4 supported fields only
- fail preflight if any referenced variable is missing; display the failure inline below the URL bar as a sticky notice (see §7.10) that lists the missing variable names and the scope chain that was checked
- source request/environment/workspace variable rows from the active collection store and workspace state, not from SQLite-only assumptions
- keep history and logs secret-safe:
  - do not persist resolved secret values into SQLite or blobs
  - continue using Phase 3 redaction paths for request summaries
- keep Git completely out of the request resolution path
  - branch checkout may change the underlying files
  - variable resolution still reads only through collection/workspace state

Definition of done:

- request sends no longer treat `{{var}}` as literal text when the variable exists
- missing variables fail fast and clearly with the correct inline UI placement
- `ResolvedRequest` is the only type that flows into the HTTP execution layer; `RequestItem` cannot be passed directly to it
- resolution precedence is unit-tested

## Slice 7: Delete Semantics and Draft Cleanup

Purpose: make destructive mutations complete rather than partial.

Note: this slice has two logically independent concerns. Tab cleanup and selection clearing work correctly regardless of whether Slice 5 has landed. Active-environment invalidation depends on Slice 5. Both are in scope here, but if Slice 7 needs to be split for scheduling reasons, the environment invalidation part has a hard dependency on Slice 5.

Tasks:

- extend delete closure logic to include draft request tabs
  - if a draft belongs to a deleted collection, close it
  - if a draft targets a deleted folder as its parent, close it
- cancel in-flight request operations before closing affected request tabs
- clear stale sidebar selection after delete
  - prefer nearest surviving ancestor
  - otherwise fall back to active tab item or no selection
- clear active environment when its environment row is deleted

Definition of done:

- deleting a parent node leaves no dangling request tabs or stale environment selection
- draft tabs behave consistently with persisted tabs during delete cascades

## Slice 8: Remaining Keyboard Actions, Observability, and Performance Audit

Purpose: finish the phase without leaving hidden interaction or idle-CPU regressions behind.

Tasks:

- add remaining tree keyboard behavior (Left/Right expansion is already wired in Slice 2):
  - `Enter` opens/focuses the selected item
  - `Delete` / platform-appropriate destructive key path triggers delete flow
  - context-menu key / alternate gesture opens item menu
- add tracing spans:
  - `tree.create`
  - `tree.rename`
  - `tree.move`
  - `tree.delete`
  - `environment.select`
  - `variable.resolve`
  - `linked_collection.reconcile`
- add counters for:
  - rejected illegal drops
  - missing variable failures
  - async entity update drops
  - `tree.catalog_reload` — incremented each time the workspace catalog is reloaded; used by the performance audit to verify no reloads occur during expand/collapse cycles
- run a focused GPUI performance audit against the tree and variable editor flows with concrete pass/fail criteria:
  - attach the GPUI frame profiler and confirm sidebar `render()` time stays under 2 ms per frame with 200+ visible rows
  - confirm zero catalog reloads occur during 10 rapid expand/collapse cycles (verify via `tree.catalog_reload` counter staying at zero)
  - confirm no idle CPU climb after 30 seconds of inactivity with the tree fully loaded
  - confirm variable row rebuild does not leak subscriptions (verify by comparing subscription count before and after 5 rebuild cycles)

Definition of done:

- remaining tree workflows are keyboard-usable
- structural mutations and variable failures are observable in traces/logs
- the Phase 4 surfaces pass the render-loop checklist from `docs/gpui-performance.md`
- all four performance audit criteria pass

## 10. Validation Gates

Required automated coverage:

- repo tests for:
  - collection storage kind persistence
  - workspace/request/environment variable persistence
  - legacy environment map JSON read compatibility
  - legacy environment map JSON idempotency: read old format → save → read again → save again produces identical output on the second round-trip
- linked collection tests for:
  - file format round-trip
  - stable ID preservation across rename/move
  - parent metadata owns sibling ordering
  - full rebuild from tracked files reproduces tree/order without cache
  - reconcile of file add/change/remove into collection state
- collection store adapter dispatch tests:
  - given a managed collection ID, `CollectionStore` resolution returns a `ManagedCollectionStore`
  - given a linked collection ID, `CollectionStore` resolution returns a `LinkedCollectionStore`
  - CRUD operations on a managed collection ID do not touch the filesystem
  - CRUD operations on a linked collection ID do not touch the managed SQLite tables
- tree tests for:
  - flat row expansion behavior
  - unified mixed sibling ordering
  - sort_order collision normalization produces unique contiguous values
  - illegal folder-into-descendant rejection
  - cross-collection move semantics within the same storage kind
- session tests for:
  - active environment persistence
  - expanded-items persistence
- request execution tests for:
  - precedence order
  - disabled variable exclusion
  - duplicate-key last-wins behavior
  - missing-variable preflight failure
- delete cleanup tests for:
  - persisted descendant tab closure
  - draft tab closure on collection/folder delete
  - active-environment invalidation on delete

Required manual validation:

- create, rename, move, reorder, and delete all tree item kinds from the UI
- create both managed and linked collections and confirm the correct storage authority is used
- drag a request across folders and across collections (within the same storage kind) and confirm stable order after restart
- drag a folder with descendants across collections (within the same storage kind) and confirm descendant request collection IDs remain correct
- switch active environments and send the same request with different resolved values
- rename a linked request file/folder through the UI and confirm tab identity/history identity remains stable
- verify the planned reconcile path treats simulated branch-switch file changes the same as ordinary disk edits
- edit variable rows rapidly and confirm the workspace catalog does not reload on each keystroke
- expand/collapse the tree repeatedly and confirm no idle CPU climb and no subscription leak symptoms

Required GPUI performance audit:

- verify `render_sidebar()` contains no mutation, subscription, spawn, or external IO
- verify variable editors clear row subscriptions before rebuild
- verify active-environment changes only notify when the selected environment actually changes
- verify hover-expand tasks are stored/cancelled explicitly
- verify no broad observer reloads the entire tree on request-tab typing

## 11. Acceptance Checklist

- [x] Tree rendering uses a flat row model with explicit expansion state
- [x] Collections support both `Managed` and `Linked` storage authority
- [x] Linked collections round-trip through a stable on-disk file format with stable IDs
- [x] Linked collection order is owned by Git-visible parent metadata, not hidden SQLite state
- [x] Linked collections can rebuild fully from tracked files without relying on cache
- [x] Linked collections define one watcher/reconcile contract for normal disk edits and future Git-driven edits
- [x] Folders and requests can be interleaved and reordered under the same parent
- [x] CRUD exists for workspace, collection, folder, request, and environment items
- [ ] Drag/drop mutations are transactional and reject illegal targets
- [x] Workspace variables, environment variables, and request-local overrides all exist
- [x] Linked collection rows display a right-aligned Git indicator without changing left-side primary icons
- [x] Hover/focus on the linked-collection Git indicator shows a popover/tooltip with root-path context and actions
- [x] Linked collections support workspace-scoped environment creation/selection flow from the UI
- [x] Active environment is session-scoped per workspace and restored on reopen
- [x] Request send path resolves variables with deterministic precedence
- [x] Missing variables fail preflight with a clear user-facing state shown inline below the URL bar
- [x] Parent deletion closes persisted and draft descendant tabs
- [x] Deleting the active environment clears stale session state cleanly
- [ ] All new strings are Fluent-based in both supported locales (`i18n/en/torii.ftl` and `i18n/zh-CN/torii.ftl`)
- [ ] Phase 4 passes the `docs/gpui-performance.md` render-loop checklist
