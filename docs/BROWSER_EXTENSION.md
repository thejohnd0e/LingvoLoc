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

This creates `apps/extension/LingvoLoc-extension-1.45.1.zip`.

## Pairing

1. Start LingvoLoc.
2. Use `Copy browser extension token` in the desktop translation controls.
3. Open the LingvoLoc extension popup and paste the token into `Pairing token`.
4. Click `Pair extension` and wait for the `PAIRED` status.
5. Select text on a page and open the extension action, or use the selection context menu; the context menu opens an in-page LingvoLoc popup with the selected text automatically.

Use the `−` and `+` controls in the popup header to adjust text size from 12px to 24px. The selected size is saved for the next popup.

The extension keeps its source and target languages independently from desktop settings. The in-page popup can be moved by its brand header, resized from its lower-right corner, and its translation can be copied with `Copy`.

The token is stored only in `chrome.storage.local`. The extension sends requests to the authenticated loopback API documented in `docs/LOCAL_API.md`.
