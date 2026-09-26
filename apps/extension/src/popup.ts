import './popup.css';
import { getSelectedText, getStatus, translate } from './api';

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
};

app.innerHTML = `
  <section class="shell">
    <header>
      <div>
        <span class="eyebrow">LOCAL TRANSLATION</span>
        <h1>LingvoLoc</h1>
      </div>
      <div class="header-tools">
        <div class="font-controls" aria-label="Text size">
          <button id="font-decrease" class="font-button" type="button" aria-label="Decrease text size">−</button>
          <span id="font-size">16px</span>
          <button id="font-increase" class="font-button" type="button" aria-label="Increase text size">+</button>
        </div>
        <span class="status" id="status">READY</span>
      </div>
    </header>
    <div id="pairing" class="pairing" hidden>
      <label class="field"><span>Pairing token</span><input id="token" type="password" autocomplete="off" placeholder="Paste token from LingvoLoc" /></label>
      <button id="pair" class="secondary" type="button">Pair extension</button>
    </div>
    <div class="row">
      <label class="field compact"><span>From</span><select id="source"><option value="auto">Auto</option><option value="en">English</option><option value="ru">Russian</option><option value="de">German</option><option value="es">Spanish</option><option value="fr">French</option></select></label>
      <label class="field compact"><span>To</span><select id="target"><option value="ru">Russian</option><option value="en">English</option><option value="de">German</option><option value="es">Spanish</option><option value="fr">French</option></select></label>
    </div>
    <label class="field"><span>Selected text</span><textarea id="text" rows="5" placeholder="Select text on a page, or paste it here"></textarea></label>
    <button id="translate" type="button">Translate locally</button>
    <output id="result" aria-live="polite"></output>
    <p class="hint">The extension sends text only to LingvoLoc on this computer.</p>
  </section>
`;

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
const result = document.querySelector<HTMLOutputElement>('#result')!;

const saved = await chrome.storage.local.get([
  'apiToken',
  'pendingSelection',
  'fontSize',
]);
state.token = typeof saved.apiToken === 'string' ? saved.apiToken : '';
state.fontSize =
  typeof saved.fontSize === 'number'
    ? Math.min(24, Math.max(12, saved.fontSize))
    : 16;
state.text =
  typeof saved.pendingSelection === 'string'
    ? saved.pendingSelection
    : await getSelectedText().catch(() => '');
if (
  typeof saved.pendingSelection === 'string' &&
  saved.pendingSelection.trim()
) {
  await chrome.storage.local.remove('pendingSelection');
}
tokenInput.value = state.token;
textInput.value = state.text;

if (state.token) {
  status.textContent = 'CHECKING';
  try {
    await getStatus(state.token);
    status.textContent = 'READY';
    pairing.hidden = true;
  } catch {
    state.token = '';
    tokenInput.value = '';
    await chrome.storage.local.remove('apiToken');
    status.textContent = 'PAIRING REQUIRED';
    pairing.hidden = false;
  }
} else {
  status.textContent = 'PAIRING REQUIRED';
  pairing.hidden = false;
}

function applyFontSize() {
  const value = `${state.fontSize}px`;
  fontSizeLabel.textContent = value;
  textInput.style.fontSize = value;
  result.style.fontSize = value;
}

function changeFontSize(delta: number) {
  state.fontSize = Math.min(24, Math.max(12, state.fontSize + delta));
  void chrome.storage.local.set({ fontSize: state.fontSize });
  applyFontSize();
}

decreaseButton.addEventListener('click', () => changeFontSize(-1));
increaseButton.addEventListener('click', () => changeFontSize(1));
applyFontSize();

tokenInput.addEventListener('input', () => {
  state.token = tokenInput.value.trim();
});
sourceInput.addEventListener('change', () => {
  state.source = sourceInput.value;
});
targetInput.addEventListener('change', () => {
  state.target = targetInput.value;
});
textInput.addEventListener('input', () => {
  state.text = textInput.value;
});
pairButton.addEventListener('click', async () => {
  const token = tokenInput.value.trim();
  if (!token) {
    status.textContent = 'ERROR';
    result.textContent = 'Paste the pairing token first.';
    return;
  }
  pairButton.disabled = true;
  status.textContent = 'CHECKING';
  result.textContent = '';
  try {
    const runtime = await getStatus(token);
    await chrome.storage.local.set({ apiToken: token });
    state.token = token;
    status.textContent = 'PAIRED';
    pairing.hidden = true;
    result.textContent = runtime.detail;
  } catch (error) {
    status.textContent = 'ERROR';
    result.textContent =
      error instanceof Error ? error.message : 'Pairing failed';
  } finally {
    pairButton.disabled = false;
  }
});
button.addEventListener('click', async () => {
  if (!state.token || !state.text.trim()) {
    result.textContent = 'Add a pairing token and text first.';
    return;
  }
  button.disabled = true;
  status.textContent = 'WORKING';
  result.textContent = '';
  try {
    const translation = await translate(
      state.token,
      state.text.trim(),
      state.source,
      state.target,
    );
    result.textContent = translation.text;
    status.textContent = `${translation.latency_ms} MS`;
  } catch (error) {
    result.textContent =
      error instanceof Error ? error.message : 'Translation failed';
    status.textContent = 'ERROR';
  } finally {
    button.disabled = false;
  }
});
