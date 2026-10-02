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

This creates `apps/extension/LingvoLoc-extension-4.0.1.zip`.

## Pairing

1. Start LingvoLoc.
2. Open **Settings** in the desktop app and press `Copy token` in the **Browser extension** section.
3. Open the LingvoLoc extension window and paste the token into `Pairing token`.
4. Click `Pair extension` and wait for the `PAIRED` status.
5. Select text on a page and click the extension button, or use the selection context menu. The toolbar button opens the same in-page LingvoLoc window with the selected text filled in; it waits for you to press **Translate**. The context-menu action opens the window and starts translation automatically using the saved target language. On pages where extensions cannot inject content (for example `chrome://` pages) a separate browser window opens instead.

Use the `−` and `+` controls in the popup header to adjust text size from 12px to 24px. The selected size is saved for the next popup.

The extension keeps its source and target languages independently from desktop settings. Click **Original text** to collapse or expand the source field; that state is remembered for the next popup. Press **Clear** to remove both the selected text and translation result without changing the languages or pairing. The in-page window can be moved by its brand header and resized from its right edge, bottom edge, or lower-right corner; the fields grow with the window and the size is remembered. Drag the divider between the fields to give the source or result more room; that split is remembered too. Its translation can be copied with `Copy`.

While a translation is running, the result area shows `Translating...` and an animated progress indicator. Each source paragraph is translated separately so the result keeps the paragraph boundaries. Translation paragraphs are displayed with a first-line indent for easier reading; the indent is visual and does not change the text sent to the model. The popup carries provider usage metadata from the desktop API and displays billed characters for DeepL.

The `⧉` button in the header opens the current text, languages, and translation in a separate browser window. That window is movable and resizable, stays open, and keeps its content when you switch tabs.

The token is stored only in `chrome.storage.local`. The extension sends requests to the authenticated loopback API documented in `docs/LOCAL_API.md`.
