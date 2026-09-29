chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: 'translate-selection',
    title: 'Translate selection with LingvoLoc',
    contexts: ['selection'],
  });
});

const POPUP_WINDOW_URL = 'popup.html?window=1';

async function openWindow() {
  await chrome.windows.create({
    url: chrome.runtime.getURL(POPUP_WINDOW_URL),
    type: 'popup',
    width: 400,
    height: 680,
  });
}

/** Shows the movable, resizable overlay in the tab; restricted pages get a real window. */
async function openTranslator(
  tab: chrome.tabs.Tab | undefined,
  selection: string,
) {
  await chrome.storage.local.set({ pendingSelection: selection });
  const stored = await chrome.storage.local.get('overlaySize');
  const savedSize = stored.overlaySize as
    { width?: number; height?: number } | undefined;
  try {
    if (!tab?.id) {
      throw new Error('No active tab');
    }
    await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      args: [
        chrome.runtime.getURL('popup.html'),
        JSON.stringify(savedSize ?? {}),
      ],
      func: overlayScript,
    });
  } catch {
    await openWindow();
  }
}

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== 'translate-selection') {
    return;
  }
  await openTranslator(tab, info.selectionText?.trim() ?? '');
});

chrome.action.onClicked.addListener(async (tab) => {
  let selection = '';
  try {
    if (tab.id) {
      const [result] = await chrome.scripting.executeScript({
        target: { tabId: tab.id },
        func: () => window.getSelection()?.toString() ?? '',
      });
      selection = result?.result?.trim() ?? '';
    }
  } catch {
    // Restricted pages cannot be read; the window opens without a selection.
  }
  await openTranslator(tab, selection);
});

function overlayScript(popupUrl: string, sizeJson: string) {
  const size = JSON.parse(sizeJson) as {
    width?: number;
    height?: number;
  };
  document.querySelector('[data-lingvoloc-overlay]')?.remove();

  const overlay = document.createElement('div');
  overlay.dataset.lingvolocOverlay = 'true';
  Object.assign(overlay.style, {
    position: 'fixed',
    top: '50%',
    left: '50%',
    zIndex: '2147483647',
    width: `${Math.max(320, size.width ?? 360)}px`,
    height: `${Math.max(420, size.height ?? 590)}px`,
    maxWidth: 'calc(100vw - 32px)',
    maxHeight: 'calc(100vh - 32px)',
    transform: 'translate(-50%, -50%)',
    borderRadius: '12px',
    overflow: 'hidden',
    boxShadow: '0 18px 50px rgb(0 0 0 / 42%)',
    background: '#0f151b',
  });

  const close = document.createElement('button');
  close.type = 'button';
  close.textContent = '×';
  close.setAttribute('aria-label', 'Close LingvoLoc');
  Object.assign(close.style, {
    position: 'absolute',
    top: '7px',
    right: '7px',
    zIndex: '1',
    width: '24px',
    height: '24px',
    padding: '0',
    border: '0',
    borderRadius: '50%',
    background: 'rgb(15 21 27 / 82%)',
    color: '#e9eef5',
    font: '20px/20px system-ui',
    cursor: 'pointer',
  });
  close.addEventListener('click', () => overlay.remove());

  const frame = document.createElement('iframe');
  frame.src = popupUrl;
  frame.title = 'LingvoLoc';
  frame.allow = 'clipboard-write';
  Object.assign(frame.style, {
    width: '100%',
    height: '100%',
    border: '0',
  });
  // Edge and corner handles: the framed page is fluid, so its fields follow the size.
  const makeHandle = (
    cursor: string,
    style: Partial<CSSStyleDeclaration>,
    growX: boolean,
    growY: boolean,
  ) => {
    const handle = document.createElement('div');
    Object.assign(handle.style, {
      position: 'absolute',
      zIndex: '2',
      cursor,
      touchAction: 'none',
      ...style,
    });
    let state:
      | { startX: number; startY: number; width: number; height: number }
      | undefined;
    handle.addEventListener('pointerdown', (event) => {
      event.preventDefault();
      state = {
        startX: event.clientX,
        startY: event.clientY,
        width: overlay.offsetWidth,
        height: overlay.offsetHeight,
      };
      handle.setPointerCapture(event.pointerId);
    });
    handle.addEventListener('pointermove', (event) => {
      if (!state) return;
      if (growX) {
        overlay.style.width = `${Math.max(320, state.width + event.clientX - state.startX)}px`;
      }
      if (growY) {
        overlay.style.height = `${Math.max(420, state.height + event.clientY - state.startY)}px`;
      }
    });
    handle.addEventListener('pointerup', (event) => {
      state = undefined;
      handle.releasePointerCapture(event.pointerId);
      void chrome.storage.local.set({
        overlaySize: {
          width: overlay.offsetWidth,
          height: overlay.offsetHeight,
        },
      });
    });
    return handle;
  };
  const rightHandle = makeHandle(
    'ew-resize',
    { top: '0', right: '0', width: '8px', height: 'calc(100% - 18px)' },
    true,
    false,
  );
  const bottomHandle = makeHandle(
    'ns-resize',
    { left: '0', bottom: '0', height: '8px', width: 'calc(100% - 18px)' },
    false,
    true,
  );
  const resizeHandle = makeHandle(
    'nwse-resize',
    {
      right: '0',
      bottom: '0',
      width: '18px',
      height: '18px',
      // Diagonal grip lines make the corner discoverable.
      background:
        'linear-gradient(135deg, transparent 0 45%, #7f93a3 45% 52%, transparent 52% 62%, #7f93a3 62% 69%, transparent 69% 79%, #7f93a3 79% 86%, transparent 86%)',
    },
    true,
    true,
  );
  let dragState:
    { startX: number; startY: number; left: number; top: number } | undefined;
  window.addEventListener('message', (event) => {
    if (event.source !== frame.contentWindow) {
      return;
    }
    if (event.data?.type === 'lingvoloc-close') {
      overlay.remove();
      return;
    }
    if (event.data?.type === 'lingvoloc-drag-end') {
      dragState = undefined;
      return;
    }
    if (event.data?.type === 'lingvoloc-drag-move') {
      if (!dragState) return;
      overlay.style.left = `${dragState.left + event.data.x - dragState.startX}px`;
      overlay.style.top = `${dragState.top + event.data.y - dragState.startY}px`;
      return;
    }
    if (event.data?.type !== 'lingvoloc-drag-start') return;
    const rect = overlay.getBoundingClientRect();
    overlay.style.transform = 'none';
    dragState = {
      startX: event.data.x,
      startY: event.data.y,
      left: rect.left,
      top: rect.top,
    };
  });
  overlay.append(close, frame, rightHandle, bottomHandle, resizeHandle);
  document.documentElement.appendChild(overlay);
}
