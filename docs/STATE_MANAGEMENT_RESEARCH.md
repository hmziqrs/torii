# GPUI State Management Research

> Research for building a Postman-like API client in GPUI (Zed's Rust UI framework).
> Covers protocols: HTTP/REST, GraphQL, WebSocket, gRPC, XML/SOAP.

---

## Table of Contents

1. [Current Codebase Analysis](#1-current-codebase-analysis)
2. [GPUI State Primitives](#2-gpui-state-primitives)
3. [Global State Patterns](#3-global-state-patterns)
4. [Entity System](#4-entity-system)
5. [Async Operations](#5-async-operations)
6. [Proposed Architecture](#6-proposed-architecture)
7. [Entity Models](#7-entity-models)
8. [Async Request Lifecycle](#8-async-request-lifecycle)
9. [WebSocket & gRPC Streaming](#9-websocket--grpc-streaming)
10. [Cross-Entity References](#10-cross-entity-references)

---

## 1. Current Codebase Analysis

### File Structure

```
src/
  main.rs              # Entry point (14 lines)
  app.rs               # App init, globals, actions, window creation (246 lines)
  root.rs              # Root layout entity (169 lines)
  sidebar.rs           # Page enum for navigation (34 lines)
  menus.rs             # Menu bar construction (125 lines)
  title_bar.rs         # Title bar + settings dropdown (165 lines)
  views/
    mod.rs             # Re-exports
    home.rs            # Home page
    about.rs           # About page
    settings.rs        # Settings page
    form_page.rs       # Form page with validation (403 lines)
themes/                # 21 JSON theme files (hot-reloaded)
```

### Existing State Patterns

| Pattern | Usage | Example |
|---------|-------|---------|
| **GPUI Globals** | App-wide singletons | `AppState` (empty placeholder), `Theme`, `GlobalState` |
| **GPUI Entities** | Reactive component state | `AppRoot`, `FormPage`, `InputState` — created with `cx.new(\|cx\| T::new(...))` |
| **File persistence** | Persisted state | `PersistedState` serialized to `target/state.json` via serde |
| **Actions** | Global keyboard shortcuts | `ToggleSearch`, `SwitchTheme`, `SelectLocale` |
| **Subscriptions** | Child entity events | `cx.subscribe()` for search input events |
| **Theme hot-reload** | Runtime theme changes | `ThemeRegistry::watch_dir()` |

### Current Dependencies

| Dependency | Purpose |
|------------|---------|
| `gpui` | Core UI framework (from Zed git repo) |
| `gpui_platform` | Platform abstraction |
| `gpui-component` | Pre-built UI components (local path) |
| `gpui-component-assets` | Bundled assets (fonts, icons) |
| `anyhow` | Error handling |
| `serde` / `serde_json` | Serialization |
| `tracing` / `tracing-subscriber` | Logging |
| `rust-i18n` | Internationalization |
| `regex` | Input validation |

---

## 2. GPUI State Primitives

GPUI provides three core state primitives:

### Global (App-wide singleton)
- Marker trait: `impl Global for MyStruct {}`
- Stored in `App.globals_by_type: FxHashMap<TypeId, Box<dyn Any>>`
- Access: `cx.global::<T>()`, `cx.set_global::<T>(value)`, `cx.update_global::<T>(fn)`
- No practical limit on number of globals
- Lifecycle: lives for the entire app lifetime

### Entity (Observable stateful model)
- Created: `cx.new(|cx| T::new(...))` returns `Entity<T>`
- Read: `entity.read(cx)` — borrow the inner value
- Write: `entity.update(cx, |this, cx| { ... })` — mutate and get context
- Weak ref: `entity.downgrade()` returns `WeakEntity<T>`
- Notify: `cx.notify()` triggers re-render after mutation
- Observe: `cx.observe(&entity, |this, entity, cx| { ... })` — react to changes
- Subscribe: `cx.subscribe(&entity, |this, event, cx| { ... })` — listen for events

### Context Types

| Context | When to use | Capabilities |
|---------|-------------|-------------|
| `App` | Global init, no window | Globals, spawn, open windows |
| `Window` | Window-specific ops | Focus, dialogs, notifications |
| `Context<T>` | Inside entity methods | All of App + entity ops, notify, observe, emit |
| `AsyncApp` | Inside `cx.spawn()` | Entity read/write, globals, spawn more tasks |
| `AsyncWindowContext` | Inside `cx.spawn_in()` | Same as AsyncApp + window operations |

---

## 3. Global State Patterns

### How the Global trait works

```rust
// The Global trait is a simple marker trait (76 lines in gpui/src/global.rs)
pub trait Global: 'static + Send + Sync {}

// Usage
pub struct HttpEngine {
    client: Arc<reqwest::Client>,
}
impl Global for HttpEngine {}
```

### How Zed uses globals

From `zed/src/main.rs` — Zed registers **15+ globals** during init. Key patterns:

```rust
// Pattern 1: Plain global (service/resource)
pub struct Client { ... }
impl Global for Client {}

// Pattern 2: Global wrapping Arc (shared ownership)
pub struct GlobalAppState(Arc<AppState>);
impl Global for GlobalAppState {}

// Pattern 3: Global wrapping Entity (reactive global state)
pub struct GlobalExtensionStore(Entity<ExtensionStore>);
impl Global for GlobalExtensionStore {}
```

### Global vs Entity decision guide

| Use Global when | Use Entity when |
|----------------|-----------------|
| Truly one-per-app (connection pools, registries) | UI observes the state (request, response, tabs) |
| Read-heavy, rarely changes | State changes frequently and triggers re-renders |
| Service/infrastructure (HTTP client, WebSocket manager) | Per-instance state (each tab has its own request) |
| Needs to be accessed from anywhere without references | Part of an ownership tree (workspace → tabs) |

### The Global-wraps-Entity pattern

For state that is both app-wide AND reactive:

```rust
pub struct GlobalStore(Entity<Store>);
impl Global for GlobalStore {}

// Any code can access:
let store = cx.global::<GlobalStore>().0.read(cx);

// Any code can update:
cx.global::<GlobalStore>().0.update(cx, |store, cx| {
    store.collections.push(new_collection);
    cx.notify();
});
```

---

## 4. Entity System

### Internal structure of Entity<T>

**Source:** `crates/gpui/src/app/entity_map.rs`

```rust
pub struct Entity<T> {
    pub(crate) any_entity: AnyEntity,
    pub(crate) entity_type: PhantomData<fn(T) -> T>,
}

pub struct AnyEntity {
    pub(crate) entity_id: EntityId,        // slotmap key (unique u64)
    pub(crate) entity_type: TypeId,         // Rust's built-in type ID
    entity_map: Weak<RwLock<EntityRefCounts>>,  // weak ref to shared ref-count map
}
```

The type parameter `T` is phantom data — it exists purely for compile-time type safety. At runtime, everything is type-erased to `AnyEntity`.

### Internal structure of WeakEntity<T>

```rust
pub struct WeakEntity<T> {
    any_entity: AnyWeakEntity,
    entity_type: PhantomData<fn(T) -> T>,
}

pub struct AnyWeakEntity {
    pub(crate) entity_id: EntityId,
    entity_type: TypeId,
    entity_ref_counts: Weak<RwLock<EntityRefCounts>>,  // does NOT increment ref count
}
```

The critical difference: `WeakEntity` does NOT increment the reference count. It is purely an identifier.

### Model<T> vs Entity<T>

**There is no `Model<T>` type in current GPUI.** The codebase exclusively uses `Entity<T>` and `WeakEntity<T>`. Older versions used "Model" terminology which was later unified to "Entity". The module docs confirm: "every model or view in the application is actually owned by a single top-level object called the App...referred to collectively as entities."

### How entity creation works (cx.new())

**Source:** `crates/gpui/src/app.rs` lines 2356-2370

```rust
fn new<T: 'static>(&mut self, build_entity: impl FnOnce(&mut Context<T>) -> T) -> Entity<T> {
    self.update(|cx| {
        let slot = cx.entities.reserve();         // 1. Reserve slot + ref_count=1
        let handle = slot.clone();                // 2. Clone handle (ref_count=2)
        let entity = build_entity(&mut Context::new_context(cx, slot.downgrade())); // 3. Build entity
        cx.push_effect(Effect::EntityCreated { ... }); // 4. Push creation effect
        cx.entities.insert(slot, entity)          // 5. Insert into EntityMap, return handle
    })
}
```

`reserve()` allocates a slotmap key with `AtomicUsize` ref count set to 1. `insert()` places `Box<T>` into a `SecondaryMap<EntityId, Box<dyn Any>>`.

**Important:** The `Context<T>` passed to the build callback is constructed with a `WeakEntity<T>`, so you can call `cx.observe()`, `cx.subscribe()`, etc. inside the constructor before the entity even exists.

### How entity.read(cx) works

1. Asserts valid context (checks `Weak<RwLock<EntityRefCounts>>` matches)
2. Records the entity ID in `accessed_entities` (used for reactive tracking)
3. Does `downcast_ref()` on the `Box<dyn Any>` stored in the SecondaryMap
4. **Panics** if the entity is currently leased (being updated)

### How entity.update(cx, |this, cx|) works — the lease system

GPUI uses a **lease-based** borrow checking system, not Rust's compile-time borrow checker:

1. `lease()` physically **removes** the `Box<dyn Any>` from the SecondaryMap and moves it onto the stack as a `Lease<T>`
2. If you try to update an entity that is already leased, it **panics** with "cannot {operation} {type} while it is already being updated"
3. `end_lease()` moves it back into the map
4. The `Lease` struct has a `Drop` impl that **panics** if not properly ended

This prevents double-mutates at runtime (similar to `RefCell` semantics but implemented with slotmap removal).

### How WeakEntity.upgrade() works

```rust
pub fn upgrade(&self) -> Option<AnyEntity> {
    let ref_counts = self.entity_ref_counts.upgrade()?;  // Can the Arc be accessed?
    let ref_counts = ref_counts.read();
    let ref_count = ref_counts.counts.get(self.entity_id)?;  // Does the slot exist?

    if atomic_incr_if_not_zero(ref_count) == 0 {
        return None;  // Entity was already dropped
    }
    // ... construct a new AnyEntity with the incremented ref count
}
```

Uses a CAS loop (`compare_exchange_weak`) for thread-safe atomic increment — prevents upgrading an entity that has already been dropped.

### Reference counting lifecycle

- **Clone** (`Entity<T>`): `fetch_add(1, SeqCst)` on the atomic counter
- **Drop** (`Entity<T>`): `fetch_sub(1, SeqCst)`, and if previous count was 1 (last reference), pushes entity_id to `dropped_entity_ids`
- **Destruction**: During `flush_effects()`, `release_dropped_entities()` removes the entity from the slotmap, removes all observers and event listeners, fires `on_release` callbacks, then drops the `Box<T>`

### Why async closures MUST use WeakEntity

- An `Entity<T>` is a strong reference that **prevents** the entity from being released
- An async task can live arbitrarily long across `.await` points
- If the task held a strong `Entity<T>`, it would create a **retain cycle**: entity owns task, task holds strong reference to entity
- `WeakEntity` does not prevent release — when the task resumes after an await, `weak.upgrade()` returns `None` if the entity was dropped, allowing graceful handling

### How cx.notify() triggers re-renders

Two paths:

1. **No window is tracking the entity:** A `Notify` effect is queued. When flushed, it calls all registered observers via `observers.retain(&emitter, |handler| handler(self))`.
2. **A window IS tracking the entity:** The window invalidators are called directly to schedule a re-render. This is the reactive rendering path — when a view's `Render::render()` method reads entity state via `entity.read(cx)`, the entity is tracked, and subsequent `notify()` calls invalidate the window.

### How cx.observe() works

```rust
pub fn observe<W>(
    &mut self, entity: &Entity<W>,
    mut on_notify: impl FnMut(&mut T, Entity<W>, &mut Context<T>) + 'static,
) -> Subscription {
    let this = self.weak_entity();
    self.app.observe_internal(entity, move |e, cx| {
        if let Some(this) = this.upgrade() {
            this.update(cx, |this, cx| on_notify(this, e, cx));
            true  // Keep subscription alive
        } else {
            false  // Observer entity was dropped, auto-remove subscription
        }
    })
}
```

Key behaviors:
- Observer is stored by `EntityId` in `App::observers`
- If the observer entity is dropped, the subscription auto-cleans (returns `false`)
- If the observed entity is dropped, the subscription also auto-cleans

### How cx.subscribe() works (typed events)

```rust
pub fn subscribe<T2, Evt>(
    &mut self, entity: &Entity<T2>,
    mut on_event: impl FnMut(&mut T, Entity<T2>, &Evt, &mut Context<T>) + 'static,
) -> Subscription
where T2: 'static + EventEmitter<Evt>, Evt: 'static,
```

- Entity must implement the marker trait `EventEmitter<E>`
- Events emitted via `cx.emit(event)` push an `Effect::Emit`
- During flush, listeners are matched by both `EntityId` AND `TypeId` of the event type
- Multiple event types per entity, multiple subscribers per event type

### Entity composition pattern (from current codebase)

```rust
pub struct AppRoot {
    active_page: Page,
    collapsed: bool,
    focus_handle: FocusHandle,
    title_bar: Entity<AppTitleBar>,
    search_input: Entity<InputState>,
    home_page: Entity<HomePage>,
    form_page: Entity<FormPage>,
    _subscriptions: Vec<Subscription>,
}

impl AppRoot {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx));
        let _subscriptions = vec![
            cx.subscribe(&search_input, |this, _input, event, cx| {
                cx.notify();
            }).detach(),
        ];
        // ...
    }
}
```

### How Zed structures entities (federated approach)

Zed's Workspace (workspace.rs lines 1318-1376) uses many smaller entities — NOT one big monolithic entity:

```rust
pub struct Workspace {
    weak_self: WeakEntity<Self>,
    center: PaneGroup,
    left_dock: Entity<Dock>,
    bottom_dock: Entity<Dock>,
    right_dock: Entity<Dock>,
    panes: Vec<Entity<Pane>>,
    active_pane: Entity<Pane>,
    status_bar: Entity<StatusBar>,
    modal_layer: Entity<ModalLayer>,
    toast_layer: Entity<ToastLayer>,
    project: Entity<Project>,
    app_state: Arc<AppState>,  // shared cross-workspace state
    // ... many more fields
}

pub struct AppState {
    pub languages: Arc<LanguageRegistry>,
    pub client: Arc<Client>,
    pub user_store: Entity<UserStore>,
    pub workspace_store: Entity<WorkspaceStore>,
    pub fs: Arc<dyn fs::Fs>,
    pub node_runtime: NodeRuntime,
    pub session: Entity<AppSession>,
}
```

AppState is shared via `Arc<AppState>` AND registered as a GPUI Global (`impl Global for GlobalAppState`).

### Recommended coordination patterns

1. **Many smaller entities, NOT one big AppState entity** — each conceptual unit is its own `Entity<T>`
2. **Parent entities own child Entity handles** — Workspace stores `Entity<Pane>`, `Entity<Dock>`, etc.
3. **Coordinate via observe/subscribe** — loose coupling between entities
4. **Use GPUI Global for cross-cutting services** — HTTP client, settings, theme
5. **Use WeakEntity in async tasks, Entity everywhere else** — prevent retain cycles

---

## 5. Async Operations

### Two Executors

From `/crates/gpui/src/executor.rs`:

| Method | Thread | Can update entities? | Signature |
|--------|--------|---------------------|-----------|
| `cx.spawn(async \|this, cx\| {})` | Foreground (UI thread) | Yes | `AsyncFnOnce(WeakEntity<T>, &mut AsyncApp)` |
| `cx.spawn_in(window, async \|this, cx\| {})` | Foreground + window | Yes + Window | `AsyncFnOnce(WeakEntity<T>, &mut AsyncWindowContext)` |
| `cx.background_spawn(async {})` | Background thread pool | **No** | `Future<Output = R> + Send + 'static` |

**Key rule**: `background_spawn` futures must be `Send + 'static` and CANNOT access entities. Chain results back to foreground.

### Foreground spawn (primary pattern)

```rust
cx.spawn(async move |this, cx| {
    // Runs on UI thread, but yields at .await points
    let result = do_async_work().await;

    // Safe to update entity — we're on the foreground
    this.update(cx, |state, cx| {
        state.data = result;
        cx.notify();
    }).ok();
}).detach();
```

### Background spawn + foreground chain (heavy work)

```rust
cx.spawn(async move |this, cx| {
    // Heavy work on background thread
    let result = cx.background_spawn(async move {
        heavy_computation().await
    }).await;

    // Back on UI thread — safe to update entities
    this.update(cx, |state, cx| {
        state.data = result;
        cx.notify();
    }).ok();
}).detach();
```

### Cancellation

Tasks are cancelled by **dropping** them:

```rust
struct RequestPanel {
    active_request: Option<Task<()>>,
}

// Cancel: just drop the task
self.active_request = None;

// For graceful detection:
let fallible = task.fallible(); // Returns None if cancelled
```

### AsyncApp capabilities

From `/crates/gpui/src/app/async_context.rs`:

- `entity.update(cx, \|state, cx\| {})` — update any entity
- `entity.read(cx)` — read any entity
- `cx.spawn(async \|cx\| {})` — spawn nested foreground tasks
- `cx.background_spawn(async {})` — spawn background tasks
- `cx.update(\|cx\| {})` — invoke closure with `&mut App`
- `cx.update_global::<G, R>(update)` — update global state
- `cx.subscribe(entity, on_event)` — subscribe to events
- `cx.open_window(...)` — open windows

---

## 6. Proposed Architecture

### Design Principles

1. **Globals** for app-wide singletons (services, registries, configuration)
2. **Entities** for stateful models the UI observes (requests, tabs, collections)
3. **Weak references** in async closures (preventing retain cycles)
4. **Events/observers** for reactive coordination between entities
5. **Owned data clones** before async boundaries (no entity borrows across `.await`)

### Module Structure

```
src/
  main.rs
  app.rs                        # init(), globals, actions, window creation

  state/
    mod.rs                      # State initialization
    http_engine.rs              # Global: Arc<HttpClient> + send logic
    ws_engine.rs                # Global: WebSocket connection manager
    grpc_engine.rs              # Global: gRPC client manager
    store.rs                    # Global: AppStore (collections, envs, history)

  models/
    mod.rs
    request.rs                  # HttpRequest entity model
    response.rs                 # HttpResponse entity model
    collection.rs               # Collection + CollectionFolder models
    environment.rs              # Environment + Variable models
    history.rs                  # HistoryEntry model
    auth.rs                     # AuthConfig model
    ws_session.rs               # WebSocket session + message stream
    grpc_session.rs             # gRPC session model

  workspace/
    mod.rs                      # Workspace entity (central coordinator)
    tab_manager.rs              # TabManager entity (open tabs, active tab)
    tab.rs                      # RequestTab entity (one per open request)

  views/
    mod.rs
    sidebar/
      collection_tree.rs        # Collections sidebar
      environment_switcher.rs   # Environment dropdown
      history_list.rs           # History sidebar
    request/
      request_panel.rs          # Combined request builder
      url_bar.rs
      headers_editor.rs
      body_editor.rs
      auth_editor.rs
      params_editor.rs
    response/
      response_panel.rs         # Combined response viewer
      body_viewer.rs
      headers_viewer.rs
      timeline.rs
    tabs/
      tab_bar.rs                # Tab strip
    ws/
      ws_panel.rs               # WebSocket send/receive panel
    grpc/
      grpc_panel.rs             # gRPC request panel

  persistence/
    mod.rs
    collection_store.rs         # Serialize/deserialize collections to disk
    environment_store.rs        # Serialize/deserialize environments
    history_store.rs            # Serialize/deserialize history
```

### State Ownership Tree

```
GLOBAL STATE:
  HttpEngine (Global)         — shared reqwest::Client connection pool
  WsEngine (Global)           — WebSocket connection manager
  GrpcEngine (Global)         — gRPC channel manager
  AppStore (Global)           — persisted data
    ├── Vec<Entity<Collection>>
    ├── Vec<Entity<Environment>>
    ├── Vec<Entity<HistoryEntry>>
    └── active_environment_id: Option<EntityId>

ENTITY TREE:
  Workspace (Entity)           — per-window coordinator
    ├── Entity<TabManager>
    │     └── Vec<Entity<RequestTab>>
    │           ├── Entity<RequestModel>
    │           ├── Entity<ResponseModel>
    │           └── _send_task: Option<Task<()>>
    ├── active_environment: Option<Entity<Environment>>
    ├── sidebar_search: String
    └── sidebar_collapsed: bool
```

### Global vs Entity Decision Table

| State | Scope | Why |
|-------|-------|-----|
| `HttpEngine` | GLOBAL | One connection pool shared by all tabs |
| `WsEngine` | GLOBAL | WebSocket manager shared by all tabs |
| `GrpcEngine` | GLOBAL | gRPC channel manager shared by all |
| `AppStore` | GLOBAL | Single source of truth for persisted data |
| `Workspace` | ENTITY | Per-window, owns tabs and UI state |
| `TabManager` | ENTITY | Owned by Workspace, manages tab list |
| `RequestTab` | ENTITY | Per-tab, owns request + response |
| `RequestModel` | ENTITY | Per-tab, mutable request definition |
| `ResponseModel` | ENTITY | Per-tab, last response received |
| `WsSession` | ENTITY | Per-connection, holds message stream |
| `GrpcSession` | ENTITY | Per-call, stateful |
| `Environment` | ENTITY | Mutable, referenced by Workspace |
| `Collection` | VALUE | Stored in AppStore, cloned into tabs |
| `HistoryEntry` | ENTITY | Created per request, stored in global |

---

## 7. Entity Models

### Request Model

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum HttpMethod {
    Get, Post, Put, Patch, Delete, Head, Options,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BodyKind {
    None,
    Json(String),
    Xml(String),
    FormUrlencoded(HashMap<String, String>),
    FormData(Vec<FormField>),
    Raw(String, String),           // (content_type, body)
    Binary(Vec<u8>),
    GraphQL(GraphQLPayload),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyValue {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub description: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AuthConfig {
    None,
    Basic { username: String, password: String },
    Bearer { token: String },
    ApiKey { key: String, value: String, add_to: ApiKeyLocation },
    OAuth2 { access_token: String, token_type: String },
    Digest { username: String, password: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RequestModel {
    pub id: RequestId,
    pub name: String,
    pub method: HttpMethod,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub query_params: Vec<KeyValue>,
    pub body: BodyKind,
    pub auth: AuthConfig,
    pub pre_request_script: Option<String>,
    pub post_response_script: Option<String>,
}
```

### Response Model

```rust
#[derive(Clone, Debug)]
pub enum RequestLifecycle {
    Idle,
    Sending,
    Waiting,
    Receiving,
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Clone, Debug)]
pub enum ResponseBody {
    Pending,
    Text(String),
    Json(serde_json::Value),
    Binary(Vec<u8>),
    Error(String),
}

#[derive(Clone, Debug)]
pub struct ResponseModel {
    pub request_id: RequestId,
    pub status_code: Option<u16>,
    pub status_text: Option<String>,
    pub headers: Vec<(String, String)>,
    pub body: ResponseBody,
    pub timing: TimingInfo,
    pub lifecycle: RequestLifecycle,
    pub size_bytes: usize,
    pub error: Option<String>,
}
```

### Collection Model

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    pub description: Option<String>,
    pub folders: Vec<CollectionFolder>,
    pub requests: Vec<RequestId>,
    pub auth: Option<AuthConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CollectionFolder {
    pub id: FolderId,
    pub name: String,
    pub folders: Vec<CollectionFolder>,  // recursive nesting
    pub requests: Vec<RequestId>,
    pub auth: Option<AuthConfig>,
}
```

### Environment Model

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Environment {
    pub id: EnvironmentId,
    pub name: String,
    pub variables: HashMap<String, String>,
}

impl Environment {
    pub fn resolve(&self, template: &str) -> String {
        // Replaces {{variable_name}} with values
    }
}
```

### WebSocket Session Model

```rust
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WsConnectionState {
    Disconnected,
    Connecting,
    Connected,
    Closing,
    Closed(String),
    Error(String),
}

pub struct WsSession {
    pub id: String,
    pub url: String,
    pub headers: Vec<KeyValue>,
    pub state: WsConnectionState,
    pub messages: Vec<WsMessage>,
    pub message_count: usize,
    _connection_task: Option<gpui::Task<()>>,
}
```

### gRPC Session Model

```rust
pub struct GrpcSession {
    pub id: String,
    pub server_url: String,
    pub method: Option<GrpcMethod>,
    pub request_metadata: Vec<KeyValue>,
    pub request_body: String,
    pub response_body: Option<String>,
    pub state: RequestLifecycle,
    _call_task: Option<gpui::Task<()>>,
}
```

---

## 8. Async Request Lifecycle

### Data Flow: "Send Request"

```
[User clicks Send]
     |
     v
RequestTab::send_request() called
     |
     1. Set lifecycle = Sending
     2. cx.notify() → UI shows loading spinner
     3. Reset response entity
     4. Clone request data (owned value, no entity borrow in async)
     5. Get WeakEntity<Self> for async callback
     |
     v
cx.spawn(async move |this, cx| {
     |
     A. this.update(cx, |tab| tab.lifecycle = Waiting; cx.notify())
     |
     B. Optional: cx.background_spawn(async { heavy_work }) for CPU-intensive prep
     |
     C. HTTP client sends request (.await yields to UI event loop)
     |
     D. On success:
        this.update(cx, |tab, cx| {
            tab.response.update(cx, |resp, cx| {
                resp.status_code = Some(status);
                resp.body = ResponseBody::Text(body);
                resp.lifecycle = Completed;
                cx.notify();
            });
            tab.lifecycle = Completed;
            cx.emit(RequestTabEvent::ResponseReceived);
            cx.notify();
        });
     |
     E. On error:
        this.update(cx, |tab, cx| {
            tab.lifecycle = Failed(error);
            cx.notify();
        });
})
     |
     v
Task stored in self._send_task for cancellation
```

### Key Rules

1. **Clone data before async** — never hold entity references across `.await`
2. **Always use `WeakEntity<T>`** in async closures (provided by `cx.spawn()`)
3. **Always handle `Result` from `weak_entity.update()`** — entity may be dropped
4. **Call `cx.notify()` after every state mutation** — triggers re-render
5. **Store `Task<()>` on entity** — drop to cancel

---

## 9. WebSocket & gRPC Streaming

### WebSocket Pattern

```rust
fn connect(&mut self, cx: &mut Context<Self>) {
    self.state = WsConnectionState::Connecting;
    cx.notify();

    let this = cx.entity().downgrade();
    let url = self.url.clone();

    let task = cx.spawn(async move |this, cx| {
        let ws = tokio_tungstenite::connect_async(&url).await;

        this.update(cx, |session, cx| {
            session.state = WsConnectionState::Connected;
            cx.notify();
        }).ok();

        // Read loop — each .await yields to UI
        while let Some(msg) = ws.next().await {
            this.update(cx, |session, cx| {
                session.messages.push(msg);
                session.message_count += 1;
                cx.notify();  // Per-message UI update
            }).ok();
        }

        this.update(cx, |session, cx| {
            session.state = WsConnectionState::Closed("Done".into());
            cx.notify();
        }).ok();
    });

    self._connection_task = Some(task);
}
```

### gRPC Streaming

Same pattern as WebSocket but using gRPC stream types. Server-streaming and bidirectional calls follow the same `cx.spawn()` + read loop approach.

---

## 10. Cross-Entity References

### Reference Strategy

```
Workspace --> TabManager          : Strong Entity<T> (ownership)
TabManager --> RequestTab[]       : Strong Vec<Entity<RequestTab>> (ownership)
RequestTab --> RequestModel       : Strong Entity<RequestModel> (ownership)
RequestTab --> ResponseModel      : Strong Entity<ResponseModel> (ownership)
RequestTab --> _send_task         : Owned Task<()> (drop = cancel)
WsSession --> _connection_task    : Owned Task<()> (drop = disconnect)

Workspace --> Environment         : Shared Entity<Environment> (reference, AppStore owns)
AppStore --> Collection[]         : Strong Entity<Collection> (ownership)
AppStore --> Environment[]        : Strong Entity<Environment> (ownership)
AppStore --> HistoryEntry[]       : Strong Entity<HistoryEntry> (ownership)

Async closures --> any entity     : ALWAYS WeakEntity<T>
Collection --> RequestId          : Value (String), not entity ref (decoupled)
```

### Key Principle

> If it changes and the UI needs to react to it, it should be an `Entity<T>`.
> If it is truly one-per-app and read-heavy, it is a `Global`.
> Plain data can be embedded inside entities as value types.

---

## Source Files Referenced

- `/crates/gpui/src/global.rs` — Global trait definition (76 lines)
- `/crates/gpui/src/app.rs` — App struct with globals HashMap
- `/crates/gpui/src/app/context.rs` — Context<T> with spawn/spawn_in
- `/crates/gpui/src/app/async_context.rs` — AsyncApp, AsyncWindowContext
- `/crates/gpui/src/executor.rs` — Task, ForegroundExecutor, BackgroundExecutor
- `/crates/gpui/src/subscription.rs` — Subscription and cancellation
- `/crates/gpui/src/app/entity_map.rs` — Entity storage (SlotMap)
- `/crates/gpui/src/_ownership_and_data_flow.rs` — Architecture docs
- `/crates/workspace/src/workspace.rs` — Zed's Workspace entity pattern
- `/crates/extension_host/src/extension_host.rs` — Global wraps Entity pattern
- `/crates/zed/src/main.rs` — Full Zed init with 15+ globals
