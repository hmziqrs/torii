---
TORII MARKETING SITE BRIEF
1. What is Torii?
Torii is a desktop API client (Postman alternative) built natively in Rust on Zed's GPU-accelerated GPUI framework. It's a developer tool for debugging and testing APIs -- think of it as "Postman but built in Rust, with local-first persistence and OS-native performance."
One-liner for the hero: "The API client built for speed. Debug REST, GraphQL, WebSocket, and gRPC -- all locally, all in Rust, all with zero cloud dependency."
---
2. Feature List (Built & Working)
Core API Debugging
- REST requests: full editor with method selector (GET/POST/PUT/DELETE/PATCH/HEAD/OPTIONS), URL bar, params, auth, headers, body, scripts, settings
- Auth types: None, Basic, Bearer Token, API Key -- all with OS keychain-backed secret storage
- Body types: none, raw text, raw JSON, URL-encoded, multipart form-data, binary file uploads
- Send/Cancel/Duplicate/Save flows with proper lifecycle management
Response Viewer
- Tabbed response panel: Body, Headers, Cookies, Timing, HTML Preview
- Color-coded status bar (2xx green, 3xx blue, 4xx yellow, 5xx red)
- JSON/XML/HTML pretty-printing
- Response body search, copy to clipboard, save to file (streaming)
- Classified error display: DNS failure, connection refused, timeout, TLS errors
Workspace Organization
- Workspaces > Collections > Folders > Requests (Postman-style hierarchy)
- Two collection types: Managed (local SQLite) and Linked (filesystem-based, Git-friendly)
- Drag & drop tree reordering, cross-collection moves
- Environments with variables and deterministic resolution pipeline (workspace > environment > request)
Tab System
- Item-driven tabs: one tab per item per window
- Open-or-focus (no duplicates), dirty indicators, close-with-confirm
- Tab session persistence across restarts
History
- Global history panel per workspace with filtering (status, protocol, method, date range), grouping, and search
- Per-request history with restore capability (even for deleted source requests)
- History entry comparison (JSON diff between runs)
Security & Performance
- Zero plaintext secrets: all credentials stored in OS keychain, never in SQLite or blobs
- Bounded memory: 32 MiB per-tab volatile cap, large responses spill to disk (blob store)
- Streaming file uploads for >100 MB files (no full RAM load)
- Crash recovery and orphan blob cleanup
Developer Experience
- Keyboard shortcuts: Cmd+S save, Cmd+Enter send, Esc cancel, Cmd+D duplicate, Cmd+L focus URL, Cmd+N new request, Cmd+W close tab
- i18n: Full English + Simplified Chinese localization (Fluent-based)
- Themes: Hot-reloadable theme system, font size, border radius, dark/light mode
- HTML Preview: On-demand WebView rendering for HTML responses
Tech Highlights (for a "Built with Rust" angle)
- Built on GPUI (Zed editor's UI framework) -- GPU-accelerated, sub-millisecond rendering
- SQLite WAL for structured data, blob store for large payloads
- reqwest HTTP transport with trait-based abstraction (mockable for testing)
- Keyring OS credential storage (never stores secrets in plaintext)
- Blake3 content hashing, zstd compression-ready
- UUIDv7 sortable IDs throughout
---
3. Upcoming / In Progress (Future Features)
- GraphQL request editor (query editor, variables, operation picker)
- WebSocket client (connect/disconnect, send message, transcript viewer)
- gRPC client (unary + streaming, proto reflection)
- Git integration for linked collections (branch/status/commit/push/pull)
- File watching for linked collections (auto-reconcile external changes)
- Visual parity pass (Postman-grade UI fidelity)
- Multi-window edit conflict handling
---
4. Target Audience
- Backend/fullstack developers who test APIs daily
- Developers who don't want cloud-dependent tools (no account required, all local)
- Developers who want a native, fast desktop tool (not Electron)
- Teams using Git-backed API collections
- Rust enthusiasts and developers who care about performance
- Postman users looking for a lightweight, local-first alternative
---
5. Design & Vibe Reference
Based on the sites you referenced (voidzero.dev, vite.dev, vitest.dev, rolldown.rs, oxc.rs, viteplus.dev):
Common design language:
- Dark backgrounds with subtle gradient glows/blurs
- Floating 3D geometric shapes (Three.js) -- subtle, not distracting
- Terminal/code snippets in the hero section
- Clean, high-contrast monospace typography for code
- Framework/tech logos in a "powered by" or "trusted by" grid
- Testimonial cards with developer quotes and Twitter avatars
- Stats: star counts, contributor counts, weekly downloads
- "Get Started" CTA with copy-paste install command
- Section dividers with skewed/angled backgrounds
- Smooth scroll-triggered animations
- Footer with logo, product links, social links, copyright
Three.js elements to include (subtle, non-distracting):
- Floating geometric shapes (octahedrons, torii gate silhouette) slowly rotating in the hero
- Particle field / grid lines in the background
- Subtle parallax on scroll
- Code editor-like terminal with animated typing effect
Color palette inspiration (from the reference sites):
- VoidZero: #08060D (near-black), white text, with an orange/amber accent (#FFA400-ish)
- Consider: deep slate/charcoal (#0D1117 or #0A0A0A), with a distinctive accent (maybe a vermillion/red-orange for "torii gate" theme -- #E74C3C or #FF6B35)
- Secondary accent: teal/cyan for code highlights
---
6. Pages to Build
Page
Home (/)
Features (/features)
Docs (/docs)
Download (/download)
Blog (/blog)
Pricing (/pricing)
Auth (/login, /signup)
About (/about)
GitHub (link)
---
### 7. Tech Stack for the Website
As you specified:
- **Framework**: SvelteKit
- **UI Components**: shadcn-svelte
- **3D**: Three.js (via Threlte for Svelte, or vanilla three.js)
- **Styling**: Tailwind CSS
- **Auth**: better-auth (your skill) or Lucia Auth
- **Database**: (for auth/users) SQLite/D1 or similar
---
8. Key Copy/Messaging Ideas
Hero headline options:
- "The API client built for speed."
- "Debug APIs at the speed of Rust."
- "Your APIs. Your machine. Zero cloud."
- "The Postman alternative that runs at native speed."
Tagline:
- "A desktop API client for REST, GraphQL, WebSocket, and gRPC. Built in Rust. Local-first. Open source."
Key selling points (bullet style):
- Rust-powered performance -- GPU-accelerated UI, sub-millisecond response rendering
- Local-first, zero cloud dependency -- your data stays on your machine
- Secrets never touch disk -- OS keychain integration, no plaintext credentials
- Git-friendly collections -- store your API specs alongside your code
- Full protocol support -- REST, GraphQL, WebSocket, gRPC in one tool
- Bounded memory -- handles 100MB+ responses without breaking a sweat
---
9. Reference Site Notes (VoidZero ecosystem)
I analyzed all 6 reference sites. Key takeaways:
voidzero.dev:
- Hero: "The JavaScript Tooling Company" + "Making JavaScript developers more productive than ever before."
- Logo wall of trusted companies (Shopify, OpenAI, Framer, Linear, Hugging Face, Cloudflare, Mercedes)
- Open source projects showcase: Vite+, Vite, Vitest, Rolldown, Oxc -- each with logo, description, GitHub link, stars, contributors
- Stats: "0+ Weekly NPM downloads", "0+ GitHub Stars", "0+ Contributors"
- Footer: Product, OSS, Company, Social sections
viteplus.dev:
- Hero: "The Unified Toolchain for the Web" + code install snippet
- Feature sections: manages runtime/package manager, simplifies development, powers frameworks
- Framework logo grid (React, Vue, Svelte, Solid, +20 more)
- Stats badges (npm downloads, GitHub stars)
- "Everything you need in one tool" tabbed interface (dev & build / check / test / run / pack)
vite.dev / rolldown.rs / oxc.rs:
- Clean documentation-style landing pages
- Code snippets in hero
- "Trusted by the world's best teams" logo walls
- Feature panels with terminal screenshots
- Framework ecosystem grids
- Testimonials from well-known developers
- "Free & open source" section with sponsor grids
Common anti-patterns to AVOID (things none of these sites do):
- No carousels/sliders
- No autoplay videos (videos are click-to-play)
- No heavy animations that distract from reading
- No cookie banners that block the page
- No "pricing" pages with complex comparison tables (these are open source tools)
---
Action Items for the AI Agent
When you feed this to an agent, specify:
1. Build SvelteKit project with shadcn-svelte initialized
2. Set up Three.js via @threlte/core and @threlte/extras (Svelte-native Three.js)
3. Implement the pages listed above
4. Use the VoidZero aesthetic: dark background, gradient blurs, subtle floating 3D shapes, terminal mockups
5. Include a Torii gate (⛩️) motif -- this is literally what "torii" means
6. Add auth (login/signup) using better-auth or Lucia
7. Make the hero interactive with a Three.js scene (rotating geometric shapes or a torii gate 3D model)
8. Use the feature list from Section 2 above as content
9. Stats to feature: "Built in Rust", "GPU-accelerated", "100% local", "Zero cloud dependency"
