/**
 * Removes decoration that repeats what the result card already shows:
 * the XDXF headword key (`<k>`) and angle-bracketed sense markers such as `< I >`.
 */
function tidyDictionaryArticle(document: Document): void {
  for (const key of document.body.querySelectorAll('k')) key.remove();

  const text = (element: Element | null) => element?.textContent?.trim() ?? '';
  for (const open of [...document.body.querySelectorAll('*')]) {
    if (!open.isConnected || text(open) !== '<') continue;
    const marker = open.nextElementSibling;
    const close = marker?.nextElementSibling ?? null;
    if (
      marker &&
      close &&
      text(marker).length > 0 &&
      text(marker).length <= 4 &&
      text(close) === '>'
    ) {
      close.remove();
      marker.remove();
      open.remove();
    }
  }
}

export function sanitizeDictionaryHtml(value: string): string {
  const document = new DOMParser().parseFromString(value, 'text/html');
  tidyDictionaryArticle(document);
  for (const element of document.body.querySelectorAll('*')) {
    if (
      ['SCRIPT', 'STYLE', 'IFRAME', 'OBJECT', 'EMBED'].includes(element.tagName)
    ) {
      element.remove();
      continue;
    }
    if (['AUDIO', 'SOURCE', 'IMG'].includes(element.tagName)) {
      const src = element.getAttribute('src') ?? '';
      const hasDeferredMedia = element.hasAttribute(
        'data-dictionary-media-resource',
      );
      if (
        !hasDeferredMedia &&
        !/^data:(audio|image)\/[a-z0-9.+-]+;base64,/i.test(src)
      ) {
        element.remove();
        continue;
      }
    }
    for (const attribute of [...element.attributes]) {
      if (attribute.name.toLowerCase().startsWith('on')) {
        element.removeAttribute(attribute.name);
      }
      if (
        attribute.name === 'href' &&
        !/^(https?:|mailto:|#)/i.test(attribute.value)
      ) {
        element.removeAttribute(attribute.name);
      }
    }
  }
  return document.body.innerHTML;
}
