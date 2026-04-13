# UI Cleanup Plan

> Scope: visual polish pass on sidebar and request editor
> Date: 2026-04-13
> References: `docs/phase-3.5.md`, `torii.png` (current), `postman.png` (reference)

## 1. Goal

The current UI has several layout and styling issues that make it feel rough compared to the
target "old Postman" aesthetic: clean typographic hierarchy, compact rows, unambiguous
selection states, and no visual noise. This document catalogues every issue and prescribes a
concrete fix for each.

---

## 2. Sidebar

### 2.1 Collapsed state must show label + icon (not icon-only)

**Current behaviour**
`Sidebar::collapsed(true)` is passed when `window_layout.sidebar_collapsed == true`. The
`gpui-component` `SidebarMenuItem` implementation hides the label entirely when it receives the
`collapsed` flag — showing only a centred icon at 48 px wide.

**Target behaviour**
When the sidebar is "collapsed" the user should still read item names. The sidebar should
shrink to a narrower but still legible width (~140 px) while keeping labels visible.

**Fix**
- Remove `Sidebar::collapsed(...)` — always pass `collapsed(false)`.
- Adjust the resizable panel `size` / `size_range` to use a narrower min/default when the
  "collapsed" toggle is active (e.g. 140 px) and a normal range (180–420 px) otherwise.
- Keep the `sidebar_collapsed` flag in `WindowLayoutState` as the toggle signal; it just no
  longer feeds into `Sidebar::collapsed`.

```rust
// root.rs render_sidebar — before
Sidebar::new("app-sidebar")
    .collapsed(self.session.read(cx).window_layout.sidebar_collapsed)

// after
Sidebar::new("app-sidebar")
    // collapsed(false) is the default; labels remain visible at all widths
```

```rust
// root.rs Render — resizable_panel size before
.size(px(if sidebar_collapsed { 48. } else { sidebar_width_px }))
.size_range(
    px(if sidebar_collapsed { 48. } else { 180. })
        ..px(if sidebar_collapsed { 48. } else { 420. }),
)

// after — narrower but still label-bearing
.size(px(if sidebar_collapsed { 140. } else { sidebar_width_px }))
.size_range(
    px(if sidebar_collapsed { 140. } else { 180. })
        ..px(if sidebar_collapsed { 140. } else { 420. }),
)
```

---

### 2.2 Tree item highlight must be full width

**Current behaviour**
When hovering or selecting a `SidebarMenuItem` inside a tree (collections/folders/requests),
the highlight band is only as wide as the text content. This looks broken — visible gap on the
right side.

**Root cause**
`SidebarMenuItem` renders its inner row with `h_flex()` but the outer element does not stretch
to fill the sidebar width. The rendered element width is content-driven, not parent-driven.

**Fix**
Wrap every `SidebarMenuItem` in a `div().w_full()` so the menu item's hitbox and highlight
band always span the full sidebar width. This applies to:
- workspace items (`render_sidebar`)
- collection items (`render_collection_menu_item`)
- folder items (`render_folder_menu_item`)
- request items (`render_tree_item`)
- environment items (`render_sidebar`)
- settings / about items (`render_sidebar`)

Additionally remove any explicit horizontal margin/padding that was compensating for the
content-only width. The goal is zero gap between item rows.

```rust
// example for request items in render_tree_item
div()
    .w_full()   // ← add this wrapper
    .child(
        SidebarMenuItem::new(request.name.clone())
            ...
    )
```

---

### 2.3 Show only one section at a time (Collections OR Environments, not both)

**Current behaviour**
`render_sidebar` always renders three groups simultaneously:
1. Workspaces
2. Collections (all of them in a tree)
3. Environments

This is visual overload — the sidebar has no concept of a "current section".

**Target behaviour**
Add a top-level section switcher row at the top of the sidebar (below workspaces) with two
buttons: **Collections** and **Environments**. Only the content of the active section is
rendered below. This mirrors the old Postman sidebar that had tabs for Collections /
Environments / History.

**Implementation**
- Add `sidebar_section: SidebarSection` (`Collections | Environments`) to `WindowLayoutState`
  (default: `Collections`).
- Persist it alongside `sidebar_collapsed` and `sidebar_width_px`.
- In `render_sidebar`, render the switcher row and gate the child groups:

```rust
enum SidebarSection { Collections, Environments }

// in render_sidebar
.child(render_sidebar_section_switcher(cx))
.when(section == SidebarSection::Collections, |s| {
    s.child(/* collection group */)
})
.when(section == SidebarSection::Environments, |s| {
    s.child(/* environment group */)
})
```

The switcher itself is a compact `h_flex` with two ghost buttons that become highlighted when
active. Keep the workspaces group always visible above the switcher.

---

### 2.4 Drag and drop for tree items (folders and requests)

**Status**: deferred — complex. Left for a future pass.

Rationale: GPUI's `on_drag` / `on_drop` / `drag_over` work at the element level and require
careful ordering semantics across parent-child boundaries (a request dropping _into_ a folder
vs _between_ two folders at the same level). Implementing this cleanly requires knowing the
target insertion position (before/after/into), which the current `SidebarMenuItem` API does not
expose. Defer until sidebar tree architecture is revisited.

---

## 3. Main Body — Tab Bar

### 3.1 Tab width during drag-and-drop is fixed and clips text

**Current behaviour**
The `DragTabPreview` wraps a `build_tab()` inside a `TabBar`. `build_tab` does not constrain
tab width, so the preview width matches the _original_ tab width at the time the drag started.
On narrow tabs this clips the label mid-word.

**Fix**
Give the drag preview a minimum width and let it expand to fit content:

```rust
// tab_host.rs DragTabPreview::render
build_tab(self.title.clone(), self.icon.clone(), self.selected)
    .min_w(px(120.))   // ← ensure label fits
    .max_w(px(240.))
```

Alternatively, use a fixed tab width for _all_ tabs in the bar (not just the preview) so the
layout is predictable. Fixed-width tabs avoid the problem entirely:

```rust
// build_tab — add a fixed width
fn build_tab(title: SharedString, icon: IconName, selected: bool) -> Tab {
    Tab::new()
        .w(px(160.))          // fixed tab width
        .label(title)
        ...
}
```

Preferred approach: fixed-width tabs with `text_overflow: Truncate` on the label. Postman uses
this style.

---

## 4. Main Body — Request Editor

### 4.1 Remove the Name row from the top of the editor

**Current behaviour**
The request editor opens with:
```
[Name label]   [dirty indicator]
[Name text input — large]
[Method select]  [URL input]  [Send]
[Save] [Duplicate] [Cancel] [Reload Baseline]
[Latest Run: …]  [Settings]
[Params] [Auth] [Headers] [Body] [Scripts] [Tests]
```

The "Name" input as the topmost prominent element is visually heavy. Postman puts the name in
the tab and in a breadcrumb, not as an H1-style input dominating the top of the editor.

**Fix**
- Remove the name label row and the large name input from the top of the editor.
- Keep the name editable via the Settings dialog (`open_settings_dialog`) that already exists.
- The tab title (`TabPresentation.title`) already shows the name — no information is lost.

---

### 4.2 Unify the URL bar — method, URL, and Send must be one row at equal height

**Current behaviour**
```rust
h_flex()
    .items_end()                                    // ← items_end causes height mismatch
    .child(div().w_40().child(Select::new(...)))    // select has own internal height
    .child(div().flex_1().child(Input::new(...).large()))
    .child(Button::new("request-send").primary()...)
```
`items_end` alignment + different sizing APIs cause the three elements to visually sit at
different heights.

**Fix**
Use `items_center` (or `items_stretch`) and ensure the Select, Input, and Button are all sized
identically with `.large()` / explicit `h(px(36.))`:

```rust
h_flex()
    .gap_2()
    .items_center()                                    // ← centre-align
    .h(px(36.))
    .child(
        div()
            .w(px(120.))
            .child(Select::new(&self.method_select).large()),  // ← .large() for height
    )
    .child(
        div()
            .flex_1()
            .child(Input::new(&self.url_input).large()),
    )
    .child(
        Button::new("request-send")
            .primary()
            .large()                                   // ← .large() for height parity
            .label(...)
            .on_click(...),
    )
```

---

### 4.3 Compress the action buttons row

**Current behaviour**
Save, Duplicate, Cancel, and Reload Baseline live in a separate full-width row with `.primary()` / `.outline()` / `.ghost()` styling, making them feel like primary navigation.

**Fix**
Replace the four-button row with a smaller secondary strip. Keep Save (important action).
Move Duplicate to a context menu on the tab or the Settings dialog. Keep Cancel only when a
request is actually in-flight. Drop Reload Baseline to a menu item.

Proposed layout after URL bar:

```
[Save*]   [Duplicate]   [Cancel†]   [Reload‡]   ···   [Latest Run: 200 OK • 123 ms]   [Settings]
```

Where `*` Save is shown only when dirty, `†` Cancel only when exec_status is Sending/Streaming,
`‡` Reload only when a baseline exists. All three secondary buttons use `.ghost()`. The
"Latest Run" and "Settings" stay in the same row.

This collapses two rows (action buttons + latest-run) into one compact row.

---

### 4.4 Section tab strip — use underline/highlight style, not primary button

**Current behaviour**
`section_tab_button` returns `.primary()` for the active tab — a dark filled button. All six
tabs use the same size, so the active tab looks like a nav button, not a tab selector.

**Target behaviour**
Active tab should look like a tab: same background as the content area, with a bottom border
accent (underline-style). Inactive tabs are plain text, no border, with a hover state.

**Fix**
Rewrite `section_tab_button` and `response_tab_button` in `helpers.rs`:

```rust
pub(super) fn section_tab_button(
    id: &'static str,
    label: String,
    active: bool,
    on_click: impl Fn(...) + 'static,
) -> impl IntoElement {
    div()
        .id(id)
        .px_3()
        .py_1p5()
        .text_sm()
        .cursor_pointer()
        .when(active, |el| {
            el.border_b_2()
              .border_color(cx.theme().primary)   // accent underline
              .font_weight(FontWeight::MEDIUM)
        })
        .when(!active, |el| {
            el.text_color(cx.theme().muted_foreground)
              .hover(|el| el.text_color(cx.theme().foreground))
        })
        .on_click(on_click)
        .child(label)
}
```

Note: `section_tab_button` currently takes `cx` implicitly through the listener. The signature
needs `cx: &mut Context<RequestTabView>` to access the theme, or use hard-coded colours that
match the theme (e.g. `gpui::blue()` for the underline). A simpler approach:

```rust
if active {
    Button::new(id)
        .ghost()
        .label(label)
        .border_b_2()                       // custom bottom border
        .border_color(gpui::blue())
        .on_click(on_click)
} else {
    Button::new(id)
        .ghost()
        .label(label)
        .on_click(on_click)
}
```

The border approach requires that `Button` supports chained style overrides via `Styled`. Check
if `gpui_component::Button` passes through `Styled` — if not, wrap in a `div` instead.

---

### 4.5 Remove the section content border box

**Current behaviour**
Section content is wrapped in:
```rust
v_flex()
    .p_3()
    .rounded(px(6.))
    .border_1()
    .child(section_content)
```
This creates a card/box around the params table, auth form, etc. Postman does not box the
section content — it flows directly below the tabs with a dividing line from the tab bar.

**Fix**
Remove `.p_3()`, `.rounded(px(6.))`, `.border_1()` from the section content wrapper. Keep a
small top padding (`.pt_2()`) to breathe from the tab bar. The section content goes edge-to-edge
within the editor padding.

---

### 4.6 KV editor — proper table styling with clear column lines

**Current behaviour**
`render_kv_table` wraps `DataTable::new(table).bordered(true)` in a fixed `h(px(200.))` div.
The `DataTable` / `TableDelegate` renders inputs with `.bordered(true).rounded(px(0.))` but the
visual result is disjointed — inputs appear to float inside the table rather than sitting in
well-defined cells.

**Issues observed**:
- Fixed 200 px height truncates rows when the table has 4+ entries
- No visual row separators between rows (only column header lines)
- Inputs look like free-floating boxes instead of cell contents
- Add-row button below the table is styled as `.outline()` — visually too heavy

**Fix**

1. Replace fixed `h(px(200.))` with a dynamic height that grows with rows (up to a max):

```rust
let row_height = 32.;
let header_height = 36.;
let max_rows_visible = 8;
let rows_visible = (delegate_rows.len() + 1).min(max_rows_visible); // +1 for Add Row
let table_height = header_height + rows_visible as f32 * row_height;

div()
    .w_full()
    .h(px(table_height))
    .child(DataTable::new(table).bordered(true))
```

2. In `KvTableDelegate::render_td`, remove the `.bordered(true)` from individual Input cells —
   rely on the table's own row/column lines instead:

```rust
// Key input (col 1)
1 => div()
    .px_1()
    .child(
        Input::new(&row.key_input)
            .appearance(false)   // ← no input border; table provides structure
            .bordered(false)
            .placeholder("Key"),
    )
    .into_any_element(),
```

3. Change Add-row button from `.outline()` to a smaller `.ghost()` with a `+` icon prefix:

```rust
Button::new(prefix)
    .ghost()
    .small()
    .icon(IconName::Plus)
    .label(es_fluent::localize("request_tab_kv_add_row", None))
    .on_click(...)
```

---

### 4.7 Body type — radio buttons instead of dropdown

**Current behaviour**
`render_body_editor` shows:
```rust
div().w_56().child(Select::new(&view.body_type_select))
```
A dropdown for: None / Raw Text / Raw JSON / URL Encoded / Form Data / Binary File.

**Target behaviour**
Radio buttons in a horizontal strip, Postman-style:

```
● none  ○ raw  ○ form-data  ○ x-www-form-urlencoded  ○ binary
```

For "raw", a secondary row of radio buttons selects Text vs JSON:

```
● Text  ○ JSON
```

**Implementation**

- Remove `body_type_select: Entity<SelectState<Vec<&'static str>>>` from `RequestTabView`.
- Remove the `body_type_select` subscription.
- In `render_body_editor`, replace `Select` with an `h_flex` of `Radio` buttons:

```rust
use gpui_component::radio::Radio;

h_flex()
    .gap_4()
    .child(Radio::new("body-none").label("none")
        .checked(matches!(body, BodyType::None))
        .on_click(cx.listener(|this, checked, _, cx| {
            if *checked { this.set_body_kind(BodyKind::None, cx); }
        })))
    .child(Radio::new("body-raw-text").label("raw text")
        .checked(matches!(body, BodyType::RawText { .. }))
        .on_click(cx.listener(|this, checked, _, cx| {
            if *checked { this.set_body_kind(BodyKind::RawText, cx); }
        })))
    .child(Radio::new("body-raw-json").label("raw json")
        .checked(matches!(body, BodyType::RawJson { .. }))
        .on_click(cx.listener(|this, checked, _, cx| {
            if *checked { this.set_body_kind(BodyKind::RawJson, cx); }
        })))
    .child(Radio::new("body-urlencoded").label("x-www-form-urlencoded")
        .checked(matches!(body, BodyType::UrlEncoded { .. }))
        .on_click(cx.listener(|this, checked, _, cx| {
            if *checked { this.set_body_kind(BodyKind::UrlEncoded, cx); }
        })))
    .child(Radio::new("body-formdata").label("form-data")
        .checked(matches!(body, BodyType::FormData { .. }))
        .on_click(cx.listener(|this, checked, _, cx| {
            if *checked { this.set_body_kind(BodyKind::FormData, cx); }
        })))
    .child(Radio::new("body-binary").label("binary")
        .checked(matches!(body, BodyType::BinaryFile { .. }))
        .on_click(cx.listener(|this, checked, _, cx| {
            if *checked { this.set_body_kind(BodyKind::BinaryFile, cx); }
        })))
```

- Remove `body_type_label` sub-heading ("Body Type:") — the radio strip is self-describing.
- `sync_inputs_from_draft` currently syncs the `body_type_select` selected index; remove that
  sync call after the select is removed.

**Files touched**: `request_tab.rs` (remove field + subscription + sync), `body_editor.rs`
(replace select with radio strip).

---

## 5. Breadcrumbs

**Current behaviour**: none. After the tab bar there is no path context.

**Target behaviour**
A breadcrumb line immediately below the tab bar showing:
```
Workspace Name  /  Collection Name  /  [Folder Name /]  Request Name
```

**Implementation**
- In `root.rs Render`, between `render_tab_bar(...)` and the scrollable content `div`, insert a
  `render_breadcrumbs(active_tab, &self.catalog, cx)` call.
- `render_breadcrumbs` returns a compact `h_flex` with `/`-separated spans derived from
  `catalog.find_breadcrumb_path(active_tab.item())`.
- Add `find_breadcrumb_path(item: ItemKey) -> Vec<SharedString>` to `WorkspaceCatalog`.
- Only show for Request/Folder/Collection/Environment items; return empty for Settings/About/Workspace.

```rust
fn render_breadcrumbs(active: Option<TabKey>, catalog: &WorkspaceCatalog) -> AnyElement {
    let Some(key) = active else { return div().into_any_element() };
    let parts = catalog.find_breadcrumb_path(key.item());
    if parts.is_empty() { return div().into_any_element() }

    h_flex()
        .px_4()
        .py_1()
        .gap_1()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .children(parts.iter().enumerate().map(|(i, part)| {
            h_flex()
                .gap_1()
                .when(i > 0, |el| el.child(div().child("/")))
                .child(div().child(part.clone()))
        }))
        .into_any_element()
}
```

---

## 6. Summary of Files to Touch

| File | Changes |
|------|---------|
| `src/root.rs` | Sidebar collapsed width, remove `Sidebar::collapsed`, section switcher, full-width item wrappers, breadcrumbs row |
| `src/session/window_layout.rs` | Add `sidebar_section: SidebarSection` field |
| `src/views/tab_host.rs` | Fixed tab width for drag preview (or all tabs) |
| `src/views/item_tabs/request_tab.rs` | Remove name input row, compress action row, add breadcrumbs |
| `src/views/item_tabs/request_tab/helpers.rs` | Rewrite `section_tab_button` / `response_tab_button` as underline-style |
| `src/views/item_tabs/request_tab/body_editor.rs` | Replace Select with Radio strip, remove body-type label |
| `src/views/item_tabs/request_tab/kv_editor.rs` | Dynamic table height, borderless input cells, ghost add-row button |
| `src/services/workspace_tree.rs` | Add `find_breadcrumb_path` to `WorkspaceCatalog` |
| `i18n/en/torii.ftl` | No new strings required (all labels already exist or use inline literals) |
| `i18n/zh-CN/torii.ftl` | Mirror any new keys |

---

## 7. Implementation Order

1. **Request tab layout** (§4.1 + §4.2 + §4.3 + §4.5) — highest visual impact, self-contained
2. **Section tab strip styling** (§4.4) — requires `helpers.rs` only
3. **Body type radio buttons** (§4.7) — `body_editor.rs` + `request_tab.rs`
4. **KV editor** (§4.6) — `kv_editor.rs`
5. **Sidebar collapsed width** (§2.1) — `root.rs` one-liner
6. **Sidebar section switcher** (§2.3) — `root.rs` + `window_layout.rs`
7. **Sidebar full-width highlight** (§2.2) — `root.rs` wrapper divs
8. **Tab bar fixed width** (§3.1) — `tab_host.rs`
9. **Breadcrumbs** (§5) — `root.rs` + `workspace_tree.rs`
