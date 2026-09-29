# Browser Extension

The first extension slice is a thin Chromium MV3 client. It does not run a model or store translation data outside the browser's local extension storage.

## Build

```powershell
npm install
npm run extension:build
```

Load `apps/extension/dist` through `chrome://extensions` with Developer mode enabled.

For a distributable artifact, run:

```powershell
npm run extension:package
```

This creates `apps/extension/LingvoLoc-extension-2.1.1.zip`.

## Pairing

1. Start LingvoLoc.
2. Open **Settings** in the desktop app and press `Copy token` in the **Browser extension** section.
3. Open the LingvoLoc extension window and paste the token into `Pairing token`.
4. Click `Pair extension` and wait for the `PAIRED` status.
5. Select text on a page and click the extension button, or use the selection context menu. Both open the same in-page LingvoLoc window with the selected text filled in. On pages where extensions cannot inject content (for example `chrome://` pages) a separate browser window opens instead.

Use the `−` and `+` controls in the popup header to adjust text size from 12px to 24px. The selected size is saved for the next popup.

The extension keeps its source and target languages independently from desktop settings. The in-page window can be moved by its brand header and resized from its right edge, bottom edge, or lower-right corner; the fields grow with the window and the size is remembered. Its translation can be copied with `Copy`.

The `⧉` button in the header opens the current text, languages, and translation in a separate browser window. That window is movable and resizable, stays open, and keeps its content when you switch tabs.

The token is stored only in `chrome.storage.local`. The extension sends requests to the authenticated loopback API documented in `docs/LOCAL_API.md`.
