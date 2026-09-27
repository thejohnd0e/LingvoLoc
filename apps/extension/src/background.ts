chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: 'translate-selection',
    title: 'Translate selection with LingvoLoc',
    contexts: ['selection'],
  });
});

chrome.contextMenus.onClicked.addListener(async (info, tab) => {
  if (info.menuItemId !== 'translate-selection') {
    return;
  }
  await chrome.storage.local.set({
    pendingSelection: info.selectionText?.trim() ?? '',
  });
  try {
    if (!tab?.id) {
      throw new Error('No active tab');
    }
    await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      args: [chrome.runtime.getURL('popup.html')],
      func: (popupUrl) => {
        document.querySelector('[data-lingvoloc-overlay]')?.remove();

        const overlay = document.createElement('div');
        overlay.dataset.lingvolocOverlay = 'true';
        Object.assign(overlay.style, {
          position: 'fixed',
          top: '50%',
          left: '50%',
          zIndex: '2147483647',
          width: '360px',
          height: '590px',
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
        Object.assign(frame.style, {
          width: '100%',
          height: '100%',
          border: '0',
        });
        const resizeHandle = document.createElement('div');
        Object.assign(resizeHandle.style, {
          position: 'absolute',
          right: '0',
          bottom: '0',
          zIndex: '2',
          width: '18px',
          height: '18px',
          cursor: 'nwse-resize',
        });
        let resizeState:
          | { startX: number; startY: number; width: number; height: number }
          | undefined;
        resizeHandle.addEventListener('pointerdown', (event) => {
          event.preventDefault();
          resizeState = {
            startX: event.clientX,
            startY: event.clientY,
            width: overlay.offsetWidth,
            height: overlay.offsetHeight,
          };
          resizeHandle.setPointerCapture(event.pointerId);
        });
        resizeHandle.addEventListener('pointermove', (event) => {
          if (!resizeState) return;
          overlay.style.width = `${Math.max(320, resizeState.width + event.clientX - resizeState.startX)}px`;
          overlay.style.height = `${Math.max(420, resizeState.height + event.clientY - resizeState.startY)}px`;
        });
        resizeHandle.addEventListener('pointerup', (event) => {
          resizeState = undefined;
          resizeHandle.releasePointerCapture(event.pointerId);
        });
        let dragState:
          | { startX: number; startY: number; left: number; top: number }
          | undefined;
        window.addEventListener('message', (event) => {
          if (event.source !== frame.contentWindow) {
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
        overlay.append(close, frame, resizeHandle);
        document.documentElement.appendChild(overlay);
      },
    });
  } catch {
    await chrome.action.setBadgeText({ text: '1' });
    await chrome.action.setBadgeBackgroundColor({ color: '#d6f36b' });
  }
});
