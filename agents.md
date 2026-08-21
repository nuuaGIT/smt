# Satisfactory Resource Tracker — Agent Guide

## Performance and code overview

This is a local Rust desktop application for tracking Satisfactory resource-node allocations. The application remembers the original source path, synchronizes a private copy of the active `.sav` file from that source on startup, reloads the saved state, and uses the Refresh action to detect and persist changes from the source save. If the source path is unavailable, the active save is cleared and the UI returns to the upload state instead of parsing a stale local copy.

Performance priorities, in order:

1. Keep savegame parsing off the UI thread; the current extractor scan runs in a worker thread.
2. Avoid copying or deserializing the complete save more than necessary; use a compact parsed snapshot for refresh comparisons.
3. Hash and compare streamed/chunked data for large saves where practical.
4. Keep node usage data separate from the game save so UI edits never rewrite the original `.sav`.
5. Keep rendering incremental: redraw only changed map layers or visible node markers instead of rebuilding the complete map on every edit. VSync remains disabled, but static focused scenes do not run a permanent repaint loop; interaction and animations repaint immediately, while unfocused or minimized windows skip the map canvas and throttle background parse polling.

The current code is intentionally split into a thin UI layer and a storage layer:

- `src/main.rs` — application entry point and native window setup.
- `src/app.rs` — `egui` UI, global settings menu with Node-Namen default-off, filters, node-size control, configurable “Map bei Alt-Tab nicht rendern” behavior (default on), and default-off `Rails anzeigen`, `Foundations anzeigen`, and `Belts anzeigen` toggles, plus a right-edge-triggered, width-resizable Map-Panel that closes at near-zero width and persists its width. It also owns the centralized three-color theme, no-idle-repaint rendering with uncapped interaction/animation frames, focus/minimize-aware background throttling, debug overlay, file picker actions, status/error presentation, Refresh interaction, and save-profile allocation persistence.
- `src/map.rs` — map canvas with pan/zoom up to 32x, globally configurable 50–150% node scale, optional fixed 20,000×20,000 world-coordinate grid clipped to visible bounds, visible-marker culling, default-full claimed-node usage with explicit per-node overrides, radial usage rings, savegame-bound rectangle/circle/arrow/text annotations with Ctrl+Z undo and right-click deletion, optional parsed railroad spline, contiguous top-down foundation contours built from shared edges (with animated clipped gray diagonal hatching) instead of oversized raster-cell frames, conveyor spline rendering, meter-length totals for rails and belts (`100` Unreal world units = `1 m`), cursor world-coordinate readout centered at the bottom, nearest-neighbor Resource-Well core-to-claimed-satellite Bezier links, aggregated core totals across all nearby satellites, resource totals for the right panel with remaining-capacity bars, icon-average colors, low-remaining warning badges, and drag-reorderable resource rows; unclaimed nodes contribute their full theoretical selected-miner/250% potential while claimed nodes use parsed extractor capacity, optional node-name labels (default off), local resource icons in purity-colored inner markers, occupancy-colored tracks and claimed check badges, hover tooltips, click popups, filters, and node details.
- `src/save_parser.rs` — focused save parser facade; scans actor headers for placed miners, oil pumps, resource-well extractors, pressurizers, railroad-track actors, the lightweight buildable subsystem, and conveyor-chain actors. Railroad actors are parsed off the UI thread from `mSplineData`; lightweight foundation positions/rotations and conveyor spline locations/tangents become world-space map-layer models. The uncompressed save header also supplies `play_duration_in_seconds` for the Map-Panel playtime display.
- `src/save_parser_core/` — vendored parser core used for current Satisfactory save versions; it uses lean parsing and does not retain the full object model.
- `src/world_data.rs` — resource-node domain model, exact static world-node projection input, node-method classification (Miner/Oil/Resource Well), JSON loading, and fallback demo data.
- `data/world_resource_nodes.json` — local static resource-node table used by the map.
- `data/SMT-icons/` — local WebP resource icons rendered inside the corresponding map markers.
- `data/logo.png` — embedded native window and taskbar icon loaded through the eframe viewport configuration.
- `data/map_highres.png` — local game-derived map background.
- `data/resource_nodes.json` — fallback demo input for development without world data.
- `src/storage.rs` — persistent global settings (including the language selection), save-path-bound node allocation, unclaimed-miner-tier, resource-order profiles, map rectangle/circle/arrow/text annotations, and the last-15-drawings undo history, local save copy, SHA-256 snapshots, byte-level diffing, and storage tests.
- `src/localization.rs` — dependency-free UI dictionary for English, German, French, and Spanish; the global language defaults to English and older state files receive that default automatically.
- `Cargo.toml` — Rust dependencies and package metadata.

## Intended data flow

```text
source .sav
    │
    ├─ upload ──> local active_save.sav + state.json
    │
    └─ refresh ─> parse current source
                   │
                   ├─ unchanged: keep current snapshot
                   └─ changed: update parsed snapshot and local copy
```

The parser exposes small `ParsedSaveData`, `ParsedExtractor`, `ParsedRailSegment`, `ParsedFoundation`, and `ParsedBeltSegment` models instead of leaking binary save-format details into the UI. It recognizes normal miners, oil pumps, resource-well extractors, resource-well pressurizers, placed railroad-track actors, all lightweight foundation buildables, and conveyor-chain belt segments. For each placed extractor it re-parses only the relevant object and its `InventoryPotential` component to count Power Shards and derive the maximum overclock. For each placed rail it reads `mSplineData`, including tangent vectors, and transforms the points with the actor transform. Foundations use lightweight instance transforms and inferred footprint dimensions; belts use the chain actor's saved spline elements. The map consumes `ResourceNode` from the local static world-node table and matches parsed extractor positions to stable node IDs; optional layers are replaced atomically after a save parse and cull off-screen geometry before sampling or painting. User allocations remain separate from the original save and use the same node IDs.

## Development rules

- Never modify the original source save during normal operation. The top-bar “Savegame entfernen” action only removes SMT's private active copy and clears the active profile; it never deletes or edits the original Satisfactory save.
- A new uploaded save must replace the active save only through the explicit upload action.
- Keep user allocation data in separate project state, keyed by a stable world/node identity.
- Treat an unoverridden claimed extractor as using 100% of its parsed capacity; persist explicit usage overrides separately so entering `0` remains meaningful. Keep the optional partial-usage filter global and show its warning badge in the Normal purity color `#EAD56E`.
- Sanitize node usage before rendering and after editor input; non-finite or oversized values must clamp to the parsed capacity instead of affecting visibility or radial rendering.
- Treat `state.json` as a versioned compatibility boundary: add defaults and migrations for every new field, accept unknown future fields when known fields remain compatible, and keep old unversioned files readable.
- Keep map annotations in `rectangles_by_save`; never mix them into global settings or another savegame profile.
- Keep the UI on the original egui dark visual colors with the flat, square-container style in `src/app.rs`; do not apply that container styling to map nodes or map painting. Keep map purity/status colors separate because they carry domain meaning.
- Keep focused rendering uncapped only when explicitly desired; background/minimized rendering must remain throttled to avoid competing with Satisfactory.
- Validate parser changes against real saves from the supported Satisfactory game version.
- Run `cargo fmt -- --check`, `cargo check`, and `cargo test` before handing off changes.
- Prefer small, testable modules over putting parser, persistence, and UI logic in one file.

## Current limitations

The current MVP compares the binary save copy and reports changed byte ranges. The map now loads the local 8192px game-derived map image, the static world-node table, the matching SCIM-style world-to-map projection, and parsed railroad splines. Manual usage and notes are edited in click popups, persisted by stable node ID inside a profile keyed to the uploaded save path, and restored when that save is selected again. Global app settings (filters, node labels, and debug mode) are stored separately. Maximum usage is derived from purity, extractor type, miner tier, and detected Power Shards; a Mk.3 miner on a pure node with three shards is capped at 1,200/min. Railroad rendering currently shows the saved track geometry but does not yet model switches/signals or train movement.
