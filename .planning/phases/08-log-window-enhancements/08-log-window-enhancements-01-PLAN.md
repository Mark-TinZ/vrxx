# Phase 8: Log Window Zoom Controls and Interaction Enhancements - Plan 01

## Goal
Enhance the `VrxxLogWindow` with advanced interaction features, including text search, zoom management, and export capabilities.

## Proposed Tasks
1. **Search Integration**:
   - Add a `GtkSearchEntry` to the header bar or a dedicated search bar (GtkSearchBar).
   - Implement real-time filtering of the log buffer based on the search query.
2. **Advanced Zoom Management**:
   - Add a "Zoom Reset" action (`win.zoom_reset`) to return to the default font size.
   - Implement `GtkEventControllerScroll` to support Ctrl + Mouse Wheel zooming.
3. **Log Export**:
   - Implement a "Save As..." action using `GtkFileDialog` (or `GtkFileChooserNative` for better compatibility).
   - Allow users to export the current filtered view of the logs to a `.txt` file.
4. **UI Refinements**:
   - Add tooltips to the zoom actions.
   - Ensure the search bar follows GNOME HIG patterns.

## Success Criteria
- [ ] User can search through logs with real-time feedback.
- [ ] Zoom Reset returns the log view to 10pt font.
- [ ] Ctrl + Scroll adjusts the font size dynamically.
- [ ] User can export logs to a local file.
- [ ] UI remains HIG compliant.
