# Phase 1 Native Smoke Test

## Verified

- The release executable starts successfully.
- The native window title is `LingvoLoc`.
- The NSIS installer builds successfully for x64.
- LM Studio model listing and completion were verified separately against the local endpoint.

## UI Automation Limitation

Windows UI Automation exposes the Tauri/WebView2 surface as panes and does not expose the React textareas, selects, or buttons as automation controls. An automated click-level test therefore cannot be completed with the available system UIA surface. The remaining manual check is to enter source text, select a model, translate, copy, swap languages, and use `Ctrl+Enter` in the installed application.
