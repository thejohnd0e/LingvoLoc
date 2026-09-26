chrome.runtime.onInstalled.addListener(() => {
  chrome.contextMenus.create({
    id: 'translate-selection',
    title: 'Translate selection with LingvoLoc',
    contexts: ['selection'],
  });
});

chrome.contextMenus.onClicked.addListener(async (info) => {
  if (info.menuItemId !== 'translate-selection') {
    return;
  }
  await chrome.storage.local.set({
    pendingSelection: info.selectionText?.trim() ?? '',
  });
  try {
    await chrome.action.openPopup();
  } catch {
    await chrome.windows.create({
      url: chrome.runtime.getURL('popup.html'),
      type: 'popup',
      width: 380,
      height: 620,
      focused: true,
    });
  }
});
