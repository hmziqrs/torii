# API Client Variable Highlighting Research

Date: April 20, 2026

## Goal

Understand how API clients like Postman, Insomnia, Bruno, and Yaak implement:

- variable color coding in request inputs
- autocomplete in URL and other request fields
- IDE-like behavior inside simple inputs
- whether they use a custom LSP for URL and variable-aware editing

## Executive Summary

The common pattern is not "make a normal input smarter".

The common pattern is:

1. Use a real code editor engine or code-editor-like surface even for inline inputs.
2. Force it into single-line behavior when needed.
3. Add custom parsing/tokenization for variables and placeholders.
4. Add decorations/widgets/tooltips for resolved vs unresolved values.
5. Add local completion sources from environment variables, helper APIs, template functions, and request history.

For URL bars and variable-aware request fields, the open implementations I inspected do not rely on a full custom LSP. They rely on editor-native tokenization, overlays, decorations, widgets, and custom completion providers.

## Main Finding

These products are often building a one-line IDE, not a plain text input.

That one-line IDE typically provides:

- syntax or token highlighting
- autocomplete
- hover affordances
- click actions
- link detection
- inline rendering for variables/template tags

This is much lighter than a language server and better aligned with the problem.

## Product Breakdown

## Postman

### Confirmed from public documentation

- Postman documented variable autocomplete in request-builder areas like URL, params, headers, and body.
- Postman documented variable highlighting and hover tooltips.
- Postman documented that pre-request and test script editors used Ace editor.
- Postman currently exposes a "Variable autocomplete" setting.
- Postman currently documents that empty variables are highlighted in the URL builder and other request tabs.

### Confirmed evidence

From Postman's March 7, 2017 blog post:

- In request-builder areas, typing an open curly bracket triggers autocomplete.
- In pre-request and test scripts, which used Ace, typing the first letter of a variable triggers autocomplete.
- Variables are highlighted in orange.
- Unresolved variables are highlighted in red.
- Hover tooltips show current value and scope.

From Postman's September 2, 2020 blog post:

- Script editors support autocomplete for Node.js, Node.js modules, and `pm.*`.

From Postman's 3.0 blog post:

- Postman said it had migrated "all code rendering" to AceEditor.

### What is still inferred

I did not find a public current source repository for the modern Postman request-builder editor implementation, so I cannot prove the exact engine used in today's URL builder from source.

Best inference:

- Historically, Postman definitely used Ace for script/code surfaces.
- For request-builder fields, the behavior is consistent with a custom inline editor/token/completion implementation rather than a full URL-specific LSP.

## Insomnia

### Confirmed from source

Insomnia uses CodeMirror 5 for both full editors and one-line editors.

Relevant source:

- `packages/insomnia/src/ui/components/.client/codemirror/one-line-editor.tsx`
- `packages/insomnia/src/ui/components/.client/codemirror/extensions/autocomplete.ts`
- `packages/insomnia/src/ui/components/.client/codemirror/extensions/nunjucks-tags.ts`

### Implementation details

#### One-line editor

Insomnia has a dedicated `OneLineEditor` built on CodeMirror:

- uses `CodeMirror.fromTextArea(...)`
- disables line numbers and multiline behavior
- binds `Ctrl-Space` to autocomplete
- configures `environmentAutocomplete`
- uses a Nunjucks mode when templating is enabled

#### Variable and tag awareness

Insomnia's one-line editor can enable a custom autocomplete option named `environmentAutocomplete`.
That provider supplies:

- variables
- tags
- constants
- snippets

Autocomplete is local and application-driven. It is not an LSP integration.

#### Highlighting/rendering

Insomnia also has a Nunjucks tag extension that:

- finds templating tokens in the viewport
- replaces them with DOM widgets
- renders values
- updates on hover
- supports click-to-edit behavior

This is stronger than simple syntax coloring. It is inline widget replacement.

#### Link behavior

Insomnia also has a clickable overlay extension for links rather than an LSP-backed URL model.

### Conclusion for Insomnia

Insomnia clearly implements a one-line IDE approach with:

- CodeMirror 5
- custom mode/overlay logic
- custom completion providers
- DOM widget replacement for tags

No evidence found of an LSP for URL editing.

## Bruno

### Confirmed from source

Bruno uses CodeMirror 5 and has explicit custom logic for variable highlighting and autocomplete.

Relevant source:

- `packages/bruno-app/src/components/SingleLineEditor/index.js`
- `packages/bruno-app/src/utils/common/codemirror.js`
- `packages/bruno-app/src/utils/codemirror/autocomplete.js`

### Implementation details

#### Single-line editor

Bruno's `SingleLineEditor` is a real CodeMirror instance configured as inline input:

- no line numbers
- no multiline flow by default
- special key handling
- autocomplete setup on top
- variable overlay mode enabled

#### Variable highlighting

Bruno defines a custom CodeMirror mode called `brunovariables`.
It uses `CodeMirror.overlayMode(...)` to layer variable recognition on top of a base mode.

It recognizes:

- `{{variable}}`
- prompt variables
- mock variables
- URL path parameters like `/:id`

It classifies tokens as valid or invalid and returns classes like:

- `variable-valid`
- `variable-invalid`
- `variable-prompt`

This is straightforward overlay tokenization, not semantic language intelligence.

#### Autocomplete

Bruno's autocomplete implementation is custom and context-aware.
It builds completions from:

- environment/request variables
- helper APIs like `req`, `res`, and `bru`
- custom autocomplete hints
- mock-data functions

It derives current context from the cursor and input text, then shows hints with `cm.showHint(...)`.

This is app-defined completion logic, again not LSP.

#### Link awareness

Bruno also has a custom link-aware extension that scans visible content, marks URLs, and makes them clickable.

### Conclusion for Bruno

Bruno is a very explicit example of a single-line IDE strategy:

- CodeMirror 5 editor
- custom variable overlay mode
- custom autocomplete logic
- custom URL/link handling

No LSP needed.

## Yaak

### Confirmed from source and docs

Yaak uses CodeMirror 6 and has the cleanest modern implementation of the group.

Relevant public docs:

- Yaak docs say variables can be referenced in almost any text input.
- Yaak docs say users can trigger autocomplete with `Ctrl+Space`.

Relevant source:

- `src-web/components/UrlBar.tsx`
- `src-web/components/core/Editor/Editor.tsx`
- `src-web/components/core/Editor/extensions.ts`
- `src-web/components/core/Editor/singleLine.ts`
- `src-web/components/core/Editor/twig/extension.ts`
- `src-web/components/core/Editor/twig/completion.ts`
- `src-web/components/core/Editor/twig/templateTags.ts`

### Implementation details

#### URL bar

Yaak's URL bar is backed by an `Input` component that delegates to a reusable editor with:

- `language="url"`
- `autocompleteFunctions`
- `autocompleteVariables`
- single-line usage

So the URL field is not a normal HTML input with bolted-on parsing. It is editor-backed.

#### Single-line behavior

Yaak enforces single-line behavior using a CodeMirror 6 transaction filter:

- strips newline characters from input and paste operations
- preserves cursor behavior

That is exactly the "single-line IDE" approach.

#### Templating and variable rendering

Yaak mixes a templating parser with the base language through a `twig` extension.
It then uses:

- completion sources registered via language data
- decoration/view plugins
- widgets for template tags
- click handlers for variables and missing variables

Template tags can render as widgets with different styles for:

- variables
- functions
- invalid tags

#### Autocomplete

Yaak uses CodeMirror 6 autocomplete directly.
Completions come from:

- environment variables
- template functions
- namespaces
- generic URL/text completions

This is editor-native completion, not LSP.

#### Hyperlinks and path parameters

Yaak uses CodeMirror 6 `ViewPlugin`, `MatchDecorator`, and `hoverTooltip` for:

- hyperlink decoration
- hover actions
- path parameter affordances

Again, this is custom editor extension logic, not a language server.

### Conclusion for Yaak

Yaak is the strongest example of the architecture you likely want:

- CodeMirror 6
- custom parser mixins
- single-line editor mode
- completion providers
- decorations/widgets/tooltips
- local data sources

No evidence found of URL LSP usage.

## Cross-Product Patterns

Across the products with inspectable implementations, the recurring architecture is:

### 1. Real editor engine behind "inputs"

Examples:

- Insomnia: CodeMirror 5 `OneLineEditor`
- Bruno: CodeMirror 5 `SingleLineEditor`
- Yaak: CodeMirror 6 editor-backed `Input` / URL bar

### 2. Single-line constraints added on top

Examples:

- Insomnia and Bruno configure editor behavior to behave like an inline field
- Yaak enforces single-line editing via a transaction filter

### 3. Custom variable parsing

Common tokens:

- `{{variable}}`
- `${[ variable ]}`
- `/:id`

Mechanisms used:

- CodeMirror overlays
- custom modes
- parser extensions
- syntax tree inspection

### 4. Decorations and widgets

Used for:

- resolved vs unresolved styling
- inline chips/tags
- hover metadata
- click-to-edit

### 5. Application-defined autocomplete

Completion sources usually come from:

- current environment variables
- collection/request variables
- helper APIs
- template functions
- static protocol prefixes like `http://` and `https://`
- prior request URLs/history

### 6. Link handling is local too

Visible URLs are usually detected and decorated through regex or editor plugins, then given hover/click actions.

## LSP Question

## Do they build custom LSPs for URL fields?

From the public evidence I found: mostly no.

For URL bars and variable-aware request fields, the implementations I could inspect do not use:

- Language Server Protocol
- monaco-languageclient
- VS Code language services for URL semantics

Instead they use:

- regex/token matching
- custom parser nodes
- editor overlays
- completion providers
- decorations/widgets

## Where LSP-like behavior may appear

There are places where richer language tooling is used, but these are different problems:

- GraphQL editors
- JSON/schema-aware editors
- OpenAPI/spec editors
- script editors

Even there, the inspected products leaned toward editor-specific integrations rather than a generic custom URL LSP.

## Recommendation

If the goal is variable-aware URL or header/body inputs with color coding and autocomplete:

### Best practical approach

Use CodeMirror 6 and implement:

1. a single-line extension
2. a small parser or lightweight token recognizer for your variable syntax
3. decorations for valid/invalid variables
4. optional widgets for rendered tags
5. a completion source fed by your app state
6. hover/click actions for editing or revealing variable metadata

### Why not a plain input

A normal input is weak for this because:

- token-level styling is awkward
- inline widgets are awkward
- autocomplete UX is harder to control
- selection/cursor behavior becomes fragile quickly

### Why not LSP

LSP is usually overkill for this layer because:

- URL fields are short and domain-specific
- data comes from your app state, not a filesystem/workspace language model
- completions are mostly deterministic and local
- decorations and hover UX are editor concerns, not protocol concerns

## Suggested Mental Model

Treat the URL bar as a specialized editor, not a text box.

That means:

- editor engine for interaction quality
- lightweight domain parser for variable syntax
- app-owned completions for context awareness
- decorations/widgets for visual semantics

## Sources

### Postman

- Postman variable autocomplete and highlighting: <https://blog.postman.com/autocomplete-and-tooltips-for-variables-are-here/>
- Postman script editor autocomplete: <https://blog.postman.com/postman-script-editor-autocomplete/>
- Postman 3.0 and AceEditor note: <https://blog.postman.com/postman-3-0-a-whole-new-experience/>
- Postman settings: <https://learning.postman.com/docs/getting-started/installation/settings>
- Postman variables docs: <https://learning.postman.com/docs/sending-requests/variables/variables>

### Insomnia

- Repo: <https://github.com/Kong/insomnia>
- One line editor: <https://github.com/Kong/insomnia/blob/develop/packages/insomnia/src/ui/components/.client/codemirror/one-line-editor.tsx>
- Autocomplete extension: <https://github.com/Kong/insomnia/blob/develop/packages/insomnia/src/ui/components/.client/codemirror/extensions/autocomplete.ts>
- Nunjucks tag widgets: <https://github.com/Kong/insomnia/blob/develop/packages/insomnia/src/ui/components/.client/codemirror/extensions/nunjucks-tags.ts>
- Clickable links extension: <https://github.com/Kong/insomnia/blob/develop/packages/insomnia/src/ui/components/.client/codemirror/extensions/clickable.ts>

### Bruno

- Repo: <https://github.com/usebruno/bruno>
- Single-line editor: <https://github.com/usebruno/bruno/blob/main/packages/bruno-app/src/components/SingleLineEditor/index.js>
- Variable mode: <https://github.com/usebruno/bruno/blob/main/packages/bruno-app/src/utils/common/codemirror.js>
- Autocomplete: <https://github.com/usebruno/bruno/blob/main/packages/bruno-app/src/utils/codemirror/autocomplete.js>
- Link-aware editor behavior: <https://github.com/usebruno/bruno/blob/main/packages/bruno-app/src/utils/codemirror/linkAware.js>

### Yaak

- Docs: <https://yaak.app/docs/templating/environments-and-variables>
- Repo: <https://github.com/mountain-loop/yaak>
- URL bar: <https://github.com/mountain-loop/yaak/blob/main/src-web/components/UrlBar.tsx>
- Editor core: <https://github.com/mountain-loop/yaak/blob/main/src-web/components/core/Editor/Editor.tsx>
- Base extensions: <https://github.com/mountain-loop/yaak/blob/main/src-web/components/core/Editor/extensions.ts>
- Single-line extension: <https://github.com/mountain-loop/yaak/blob/main/src-web/components/core/Editor/singleLine.ts>
- Twig extension: <https://github.com/mountain-loop/yaak/blob/main/src-web/components/core/Editor/twig/extension.ts>
- Twig completion: <https://github.com/mountain-loop/yaak/blob/main/src-web/components/core/Editor/twig/completion.ts>
- Template tag widgets: <https://github.com/mountain-loop/yaak/blob/main/src-web/components/core/Editor/twig/templateTags.ts>
- Hyperlink extension: <https://github.com/mountain-loop/yaak/blob/main/src-web/components/core/Editor/hyperlink/extension.ts>

## Confidence Notes

High confidence:

- Insomnia, Bruno, and Yaak use editor-backed inputs with custom parsing/decorations/autocomplete.
- Bruno and Yaak do not need LSP for this feature set.
- Insomnia implements inline widget/decorator behavior for templating.

Medium confidence:

- Postman's current request-builder field implementation likely follows the same broad architecture pattern.

Lower confidence:

- The exact current editor engine used by Postman's modern request URL field could not be verified from public source.
