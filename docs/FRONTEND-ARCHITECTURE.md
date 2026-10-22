# Frontend and backend structure

## Frontend assets

`src/index.html` contains markup only and loads `styles.css`, `browser-api.js`, then `app.js`. Keep new styling in CSS and new UI behavior in JavaScript instead of embedding large blocks in HTML. The current application still uses a single HTML document with page sections; assets are separated, but routes are not yet lazy-loaded as separate HTML pages.

## Data access

`window.MadarAPI.invoke(command, args)` selects Tauri's native `invoke` in desktop mode. In a browser it selects a localStorage-backed demo adapter for authentication and basic products. Unsupported commands return a clear message rather than `TypeError: invoke is not a function`. The demo adapter is not secure and is not suitable for production business data.

## Rust commands

`lib.rs` owns database connection and initialization helpers, shared state, common validation/audit helpers and the Tauri builder. Command handlers are grouped in `*_commands.rs` files and included at crate root to preserve current handler names and minimize behavioral risk during this organizational refactor. The `run` handler registration remains centralized.

## Responsive layout

The UI uses fluid grid columns, horizontal scrolling for wide data tables, mobile navigation overflow, smaller panel spacing, and 16px mobile form controls to avoid browser zoom on iOS. Verify at 320px, 390px, 768px, 1024px and 1366px widths.


## Independent page templates

Each main section now lives in `src/pages/<page>.html`. `src/index.html` contains the shared shell, navigation, authentication overlay, and shared dialogs. `src/pages-loader.js` fetches the page templates and inserts them into `#pageHost` before loading `app.js`, so the existing event wiring and permission handling continue to find the expected element IDs. This is a first separation step; templates are currently loaded together at startup, with true per-page lazy loading left for a later step.
