# Phase 7: Log Window Refactor - Plan 01

## Goal
Refactor `VrxxLogWindow` for GNOME HIG compliance, fix autoscroll layout bugs, and stabilize real-time SSE logs.

## Tasks
1. **Extract UI Definition**: Move inline XML to `src/ui/components/log_window.ui` and register in GResource.
2. **GNOME HIG Menu**: Migrate to `GtkPopoverMenu` using `gio::MenuModel` and `SimpleAction` bindings for zoom, copy, and clear actions.
3. **Fix Autoscroll**: Use `glib::idle_add_local_once` to ensure scrolling happens after GTK layout recalculation.
4. **Stabilize SSE Logs**: Improve reconnection logging and error handling in the SSE stream receiver.

## Success Criteria
- [x] UI definition extracted to `.ui` file.
- [x] Menu refactored to use Actions and PopoverMenu.
- [x] Autoscroll is perfectly smooth and reliable.
- [x] SSE logs are robust with visible error reporting.
- [x] Code passes clippy and tests.
