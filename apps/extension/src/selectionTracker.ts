const blockTags = new Set([
  'ADDRESS',
  'ARTICLE',
  'ASIDE',
  'BLOCKQUOTE',
  'DIV',
  'FIGCAPTION',
  'H1',
  'H2',
  'H3',
  'H4',
  'H5',
  'H6',
  'HEADER',
  'LI',
  'P',
  'PRE',
  'SECTION',
  'TR',
]);

function readSelection() {
  const selection = window.getSelection();
  if (!selection?.rangeCount) return '';
  const fragment = selection.getRangeAt(0).cloneContents();
  const read = (node: Node): string => {
    if (node.nodeType === Node.TEXT_NODE) return node.textContent ?? '';
    if (node.nodeType === Node.DOCUMENT_FRAGMENT_NODE) {
      return Array.from(node.childNodes, read).join('');
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return '';
    const element = node as HTMLElement;
    if (element.tagName === 'BR') return '\n';
    const content = Array.from(element.childNodes, read).join('');
    return blockTags.has(element.tagName) ? `\n\n${content}\n\n` : content;
  };
  return read(fragment)
    .replace(/[ \t]*\n[ \t]*/g, '\n')
    .replace(/\n{3,}/g, '\n\n')
    .trim();
}

document.addEventListener('selectionchange', () => {
  void chrome.storage.local.set({
    latestSelection: {
      pageUrl: location.href,
      text: readSelection(),
    },
  });
});
