# SMT v0.1.0-rc.3

## Added

- SVG map support.
- Automatic savegame refresh with a configurable interval.
- Display of analysis duration and last update time.
- Persistent ruler annotations.
- Resource well cores with aggregated satellite usage.
- Improved savegame compatibility and backward-compatible state migration.
- Robust startup when local savegame copies are temporarily locked or cannot be synchronized.
- Detailed international number formatting for German, English, French, and Spanish.
- Plan Mode: hold Shift and drag to plan resource usage.
- Maximum possible resource values.

## Changed

- Settings reorganized into clearer sections.
- Map rendering and map layers further optimized for performance.
- Improved Windows application and window icons.
- The resource side panel now displays consumed resources instead of remaining resources.
- Improved maximum available resource calculations.

## Fixed

- Prevented startup failure when `active_save.sav` is temporarily locked by Satisfactory, antivirus software, or another process.
- Missing or outdated local savegame copies are synchronized again during the next refresh.
- Older `state.json` files continue to load correctly.
