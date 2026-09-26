import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { getCurrentWindow } from '@tauri-apps/api/window';
import App from './app/App';
import ClipboardPopup from './app/ClipboardPopup';
import './styles.css';

const Root = getCurrentWindow().label === 'popup' ? ClipboardPopup : App;

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Root />
  </StrictMode>,
);
