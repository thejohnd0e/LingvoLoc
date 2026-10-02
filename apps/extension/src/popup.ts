import './popup.css';
import { getSelectedText, getStatus, translate } from './api';
import {
  AUTO_TRANSLATE_KEY,
  clampTextSplit,
  DEFAULT_TEXT_SPLIT,
  isOriginalTextExpanded,
  isAutoTranslateRequest,
  ORIGINAL_TEXT_EXPANDED_KEY,
  TEXT_SPLIT_KEY,
} from './popupState';
import {
  formatParagraphIndents,
  splitParagraphs,
  stripParagraphIndents,
  translateParagraphs,
  formatTokenUsage,
} from './popupFormatting';

const app = document.querySelector<HTMLDivElement>('#app');
if (!app) {
  throw new Error('Popup root is missing');
}

const state = {
  token: '',
  source: 'auto',
  target: 'ru',
  text: '',
  fontSize: 16,
  textSplit: DEFAULT_TEXT_SPLIT,
  originalTextExpanded: true,
  usage: 'Token usage unavailable',
};

app.innerHTML = `
  <section class="shell">
    <header>
      <div class="brand" id="drag-handle">
        <span class="brand-mark" aria-hidden="true">L</span>
        <div>
        <span class="eyebrow">LOCAL TRANSLATION</span>
        <h1>LingvoLoc</h1>
        </div>
      </div>
      <div class="header-tools">
        <div class="font-controls" aria-label="Text size">
          <button id="font-decrease" class="font-button" type="button" aria-label="Decrease text size">−</button>
          <span id="font-size">16px</span>
          <button id="font-increase" class="font-button" type="button" aria-label="Increase text size">+</button>
          <button id="detach" class="font-button detach-button" type="button" title="Open in a separate window that stays open when you switch tabs" aria-label="Open in a separate window">⧉</button>
        </div>
          <span class="status" id="status"><i></i> READY</span>
          <div class="metrics" id="metrics" hidden aria-live="polite">
            <div class="metric"><span>Time</span><strong id="metric-time"></strong></div>
            <div class="metric"><span>Tokens</span><strong id="metric-tokens"></strong></div>
          </div>
      </div>
    </header>
    <div id="pairing" class="pairing" hidden>
      <label class="field"><span>Pairing token</span><input id="token" type="password" autocomplete="off" placeholder="Paste token from LingvoLoc" /></label>
      <button id="pair" class="secondary" type="button">Pair extension</button>
    </div>
    <div class="row pair-row">
      <label class="field compact"><span>From</span><select id="source"><option value="auto">Auto</option><option value="en">English</option><option value="ru">Russian</option><option value="de">German</option><option value="es">Spanish</option><option value="fr">French</option></select></label>
      <span class="direction" aria-hidden="true">→</span>
      <label class="field compact"><span>To</span><select id="target"><option value="ru">Russian</option><option value="en">English</option><option value="de">German</option><option value="es">Spanish</option><option value="fr">French</option><option value="it">Italian</option><option value="pt">Portuguese</option><option value="pl">Polish</option><option value="uk">Ukrainian</option><option value="zh">Chinese</option><option value="ko">Korean</option><option value="th">Thai</option></select></label>
    </div>
    <div class="text-heading"><button id="original-toggle" class="spoiler-toggle" type="button" aria-expanded="true" aria-controls="original-text-field"><span class="field-label">Original text</span><span class="spoiler-action">Hide</span><span class="spoiler-chevron" aria-hidden="true">⌄</span></button><button id="clear" class="clear-button" type="button">Clear</button></div>
    <label id="original-text-field" class="field grow"><textarea id="text" rows="5" aria-label="Original text" placeholder="Select text on a page, or paste it here"></textarea></label>
    <button id="translate" type="button"><span>Translate</span><b>Ctrl ↵</b></button>
    <div id="splitter" class="splitter" role="separator" aria-orientation="horizontal"></div>
    <div class="result-wrap grow"><div class="result-heading"><span class="result-label">TRANSLATION</span><button id="copy" class="copy-button" type="button">Copy</button></div><output id="result" aria-live="polite"></output></div>
    <p class="hint">The extension sends text only to LingvoLoc on this computer.</p>
  </section>
`;

const windowed = new URLSearchParams(window.location.search).has('window');
document.body.classList.toggle(
  'embedded',
  window.parent !== window || windowed,
);
document.body.classList.toggle('windowed', windowed);

const tokenInput = document.querySelector<HTMLInputElement>('#token')!;
const sourceInput = document.querySelector<HTMLSelectElement>('#source')!;
const targetInput = document.querySelector<HTMLSelectElement>('#target')!;
const textInput = document.querySelector<HTMLTextAreaElement>('#text')!;
const button = document.querySelector<HTMLButtonElement>('#translate')!;
const pairButton = document.querySelector<HTMLButtonElement>('#pair')!;
const pairing = document.querySelector<HTMLDivElement>('#pairing')!;
const decreaseButton =
  document.querySelector<HTMLButtonElement>('#font-decrease')!;
const increaseButton =
  document.querySelector<HTMLButtonElement>('#font-increase')!;
const fontSizeLabel = document.querySelector<HTMLSpanElement>('#font-size')!;
const status = document.querySelector<HTMLSpanElement>('#status')!;
const metrics = document.querySelector<HTMLDivElement>('#metrics')!;
const metricTime = document.querySelector<HTMLElement>('#metric-time')!;
const metricTokens = document.querySelector<HTMLElement>('#metric-tokens')!;
const result = document.querySelector<HTMLOutputElement>('#result')!;
const copyButton = document.querySelector<HTMLButtonElement>('#copy')!;
const clearButton = document.querySelector<HTMLButtonElement>('#clear')!;
const originalToggle =
  document.querySelector<HTMLButtonElement>('#original-toggle')!;
const originalTextField = document.querySelector<HTMLLabelElement>(
  '#original-text-field',
)!;
const splitter = document.querySelector<HTMLDivElement>('#splitter')!;
const shell = document.querySelector<HTMLElement>('.shell')!;

function showStatus(value: string) {
  status.hidden = false;
  metrics.hidden = true;
  status.textContent = value;
}

function showMetrics(time: number, tokens: string) {
  status.hidden = true;
  metrics.hidden = false;
  metricTime.textContent = `${time} MS`;
  metricTokens.textContent = tokens;
}

const saved = await chrome.storage.local.get([
  'detachState',
  'apiToken',
  'pendingSelection',
  'fontSize',
  'extensionSource',
  'extensionTarget',
  AUTO_TRANSLATE_KEY,
  TEXT_SPLIT_KEY,
  ORIGINAL_TEXT_EXPANDED_KEY,
]);
await chrome.action.setBadgeText({ text: '' });
state.token = typeof saved.apiToken === 'string' ? saved.apiToken : '';
state.fontSize =
  typeof saved.fontSize === 'number'
    ? Math.min(24, Math.max(12, saved.fontSize))
    : 16;
state.text =
  typeof saved.pendingSelection === 'string'
    ? saved.pendingSelection
    : windowed
      ? ''
      : await getSelectedText().catch(() => '');
state.source =
  typeof saved.extensionSource === 'string' ? saved.extensionSource : 'auto';
state.target =
  typeof saved.extensionTarget === 'string' ? saved.extensionTarget : 'ru';
state.textSplit = clampTextSplit(saved[TEXT_SPLIT_KEY]);
state.originalTextExpanded = isOriginalTextExpanded(
  saved[ORIGINAL_TEXT_EXPANDED_KEY],
);
const autoTranslate = isAutoTranslateRequest(saved[AUTO_TRANSLATE_KEY]);
await chrome.storage.local.remove(AUTO_TRANSLATE_KEY);
if (
  typeof saved.pendingSelection === 'string' &&
  saved.pendingSelection.trim()
) {
  await chrome.storage.local.remove('pendingSelection');
}
const detached = windowed
  ? (saved.detachState as
      | { text?: string; source?: string; target?: string; result?: string }
      | undefined)
  : undefined;
if (detached) {
  // Content handed over from the in-page overlay when it was moved to this window.
  state.text = detached.text ?? state.text;
  state.source = detached.source ?? state.source;
  state.target = detached.target ?? state.target;
  await chrome.storage.local.remove('detachState');
}
tokenInput.value = state.token;
textInput.value = formatParagraphIndents(state.text);
sourceInput.value = state.source;
targetInput.value = state.target;
applyTextSplit();
applyOriginalTextState();
if (detached?.result) {
  renderResult(detached.result);
}

if (state.token) {
  showStatus('CHECKING');
  try {
    await getStatus(state.token);
    showStatus('READY');
    pairing.hidden = true;
  } catch {
    state.token = '';
    tokenInput.value = '';
    await chrome.storage.local.remove('apiToken');
    showStatus('PAIRING REQUIRED');
    pairing.hidden = false;
  }
} else {
  showStatus('PAIRING REQUIRED');
  pairing.hidden = false;
}

if (autoTranslate && state.token && state.text.trim()) {
  await translateCurrentText();
}

function applyFontSize() {
  const value = `${state.fontSize}px`;
  fontSizeLabel.textContent = value;
  textInput.style.fontSize = value;
  result.style.fontSize = value;
}

function renderResult(text: string) {
  result.replaceChildren();
  for (const paragraph of splitParagraphs(text)) {
    const element = document.createElement('p');
    element.className = 'result-paragraph';
    element.textContent = paragraph;
    result.append(element);
  }
}

function getResultText(): string {
  return result.innerText.trim() || result.textContent?.trim() || '';
}

function changeFontSize(delta: number) {
  state.fontSize = Math.min(24, Math.max(12, state.fontSize + delta));
  void chrome.storage.local.set({ fontSize: state.fontSize });
  applyFontSize();
}

function applyTextSplit() {
  shell.style.setProperty('--text-split', String(state.textSplit));
  shell.style.setProperty('--text-source-grow', String(state.textSplit));
  shell.style.setProperty('--text-result-grow', String(1 - state.textSplit));
}

function applyOriginalTextState() {
  originalTextField.hidden = !state.originalTextExpanded;
  shell.classList.toggle('original-collapsed', !state.originalTextExpanded);
  originalToggle.setAttribute(
    'aria-expanded',
    String(state.originalTextExpanded),
  );
  originalToggle.setAttribute(
    'aria-label',
    state.originalTextExpanded
      ? 'Collapse original text'
      : 'Expand original text',
  );
  const spoilerAction = originalToggle.querySelector('.spoiler-action');
  if (spoilerAction) {
    spoilerAction.textContent = state.originalTextExpanded ? 'Hide' : 'Show';
  }
  originalToggle.classList.toggle('is-collapsed', !state.originalTextExpanded);
}

decreaseButton.addEventListener('click', () => changeFontSize(-1));
increaseButton.addEventListener('click', () => changeFontSize(1));
applyFontSize();

tokenInput.addEventListener('input', () => {
  state.token = tokenInput.value.trim();
});
sourceInput.addEventListener('change', () => {
  state.source = sourceInput.value;
  void chrome.storage.local.set({ extensionSource: state.source });
});
targetInput.addEventListener('change', () => {
  state.target = targetInput.value;
  void chrome.storage.local.set({ extensionTarget: state.target });
});
textInput.addEventListener('input', () => {
  state.text = stripParagraphIndents(textInput.value);
});
textInput.addEventListener('blur', () => {
  state.text = stripParagraphIndents(textInput.value);
  textInput.value = formatParagraphIndents(state.text);
});
pairButton.addEventListener('click', async () => {
  const token = tokenInput.value.trim();
  if (!token) {
    showStatus('ERROR');
    result.textContent = 'Paste the pairing token first.';
    return;
  }
  pairButton.disabled = true;
  showStatus('CHECKING');
  result.textContent = '';
  try {
    const runtime = await getStatus(token);
    await chrome.storage.local.set({ apiToken: token });
    state.token = token;
    showStatus('PAIRED');
    pairing.hidden = true;
    result.textContent = runtime.detail;
  } catch (error) {
    showStatus('ERROR');
    result.textContent =
      error instanceof Error ? error.message : 'Pairing failed';
  } finally {
    pairButton.disabled = false;
  }
});

async function translateCurrentText() {
  state.text = stripParagraphIndents(textInput.value);
  if (!state.token || !state.text.trim()) {
    result.textContent = 'Add a pairing token and text first.';
    return;
  }
  button.disabled = true;
  copyButton.disabled = true;
  showStatus('TRANSLATING');
  state.usage = 'Token usage unavailable';
  result.innerHTML =
    '<span class="loading-state" role="status"><span class="loading-dots" aria-hidden="true"><i></i><i></i><i></i></span> Translating...</span>';
  try {
    let totalLatency = 0;
    let inputTokens = 0;
    let outputTokens = 0;
    let usageParagraphs = 0;
    const paragraphs = splitParagraphs(state.text);
    const translatedText = await translateParagraphs(
      paragraphs,
      async (paragraph) => {
        const translation = await translate(
          state.token,
          paragraph,
          sourceInput.value,
          targetInput.value,
        );
        totalLatency += translation.latency_ms;
        if (
          translation.prompt_tokens != null &&
          translation.completion_tokens != null
        ) {
          inputTokens += translation.prompt_tokens;
          outputTokens += translation.completion_tokens;
          usageParagraphs += 1;
        }
        return translation.text;
      },
    );
    renderResult(translatedText);
    state.usage = formatTokenUsage(
      inputTokens,
      outputTokens,
      usageParagraphs,
      paragraphs.length,
    );
    showMetrics(totalLatency, state.usage);
  } catch (error) {
    result.textContent =
      error instanceof Error ? error.message : 'Translation failed';
    showStatus('ERROR');
  } finally {
    button.disabled = false;
    copyButton.disabled = false;
  }
}

button.addEventListener('click', () => void translateCurrentText());

clearButton.addEventListener('click', () => {
  state.text = '';
  textInput.value = '';
  result.textContent = '';
  showStatus(state.token ? 'READY' : 'PAIRING REQUIRED');
});

originalToggle.addEventListener('click', () => {
  state.originalTextExpanded = !state.originalTextExpanded;
  applyOriginalTextState();
  void chrome.storage.local.set({
    [ORIGINAL_TEXT_EXPANDED_KEY]: state.originalTextExpanded,
  });
});

let splitDrag:
  | { startY: number; startSourceHeight: number; availableHeight: number }
  | undefined;

splitter.addEventListener('pointerdown', (event) => {
  if (!document.body.classList.contains('embedded')) return;
  const sourceArea = textInput.closest<HTMLElement>('.grow');
  const resultArea = result.closest<HTMLElement>('.grow');
  if (!sourceArea || !resultArea) return;
  splitDrag = {
    startY: event.clientY,
    startSourceHeight: sourceArea.getBoundingClientRect().height,
    availableHeight:
      sourceArea.getBoundingClientRect().height +
      resultArea.getBoundingClientRect().height,
  };
  splitter.setPointerCapture(event.pointerId);
  event.preventDefault();
});

splitter.addEventListener('pointermove', (event) => {
  if (!splitDrag) return;
  const next = clampTextSplit(
    (splitDrag.startSourceHeight + event.clientY - splitDrag.startY) /
      splitDrag.availableHeight,
  );
  state.textSplit = next;
  applyTextSplit();
  event.preventDefault();
});

function finishSplitDrag(event: PointerEvent) {
  if (!splitDrag) return;
  splitDrag = undefined;
  splitter.releasePointerCapture(event.pointerId);
  void chrome.storage.local.set({ [TEXT_SPLIT_KEY]: state.textSplit });
}

splitter.addEventListener('pointerup', finishSplitDrag);
splitter.addEventListener('pointercancel', finishSplitDrag);

// Inside the page overlay (an iframe) the Clipboard API is usually blocked by the host
// page's permissions policy and logs a violation, so only the standalone popup uses it.
async function copyText(text: string) {
  if (window.parent === window) {
    try {
      await navigator.clipboard.writeText(text);
      return;
    } catch {
      // Fall back to the selection-based copy below.
    }
  }
  const helper = document.createElement('textarea');
  helper.value = text;
  helper.style.position = 'fixed';
  helper.style.opacity = '0';
  document.body.append(helper);
  helper.select();
  document.execCommand('copy');
  helper.remove();
}

copyButton.addEventListener('click', async () => {
  const text = getResultText();
  if (!text) return;
  await copyText(text);
  const original = copyButton.textContent;
  copyButton.textContent = 'Copied';
  window.setTimeout(() => {
    copyButton.textContent = original;
  }, 1200);
});

document
  .querySelector<HTMLButtonElement>('#detach')
  ?.addEventListener('click', async () => {
    await chrome.storage.local.set({
      detachState: {
        text: stripParagraphIndents(textInput.value),
        source: sourceInput.value,
        target: targetInput.value,
        result: getResultText(),
      },
    });
    await chrome.windows.create({
      url: chrome.runtime.getURL('popup.html?window=1'),
      type: 'popup',
      width: Math.max(380, window.innerWidth + 16),
      height: Math.max(520, window.innerHeight + 40),
    });
    window.parent.postMessage({ type: 'lingvoloc-close' }, '*');
  });

const dragHandle = document.querySelector<HTMLElement>('#drag-handle');
dragHandle?.addEventListener('pointerdown', (event) => {
  if (event.button !== 0 || window.parent === window) return;
  dragHandle.setPointerCapture(event.pointerId);
  window.parent.postMessage(
    { type: 'lingvoloc-drag-start', x: event.screenX, y: event.screenY },
    '*',
  );
});
dragHandle?.addEventListener('pointermove', (event) => {
  if (event.buttons & 1) {
    window.parent.postMessage(
      { type: 'lingvoloc-drag-move', x: event.screenX, y: event.screenY },
      '*',
    );
  }
});
dragHandle?.addEventListener('pointerup', (event) => {
  window.parent.postMessage({ type: 'lingvoloc-drag-end' }, '*');
  dragHandle.releasePointerCapture(event.pointerId);
});
