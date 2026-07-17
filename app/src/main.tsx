import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import { armAudioUnlock } from './lib/audio';
import './styles/global.css';
import './styles/theme-scenes.css';
import './styles/profile-personalization.css';
import './styles/profile-personalization-extra.css';
import './styles/profile-personalization-more.css';
import './styles/profile-surfaces.css';

// Déverrouillage audio armé avant tout : le premier geste (clic, frappe —
// onboarding compris) met le contexte Web Audio partagé en route, pour que
// blip, sonnerie et soundboard soient audibles dès le premier événement.
armAudioUnlock();

const root = document.getElementById('root');
if (root === null) {
  throw new Error('élément racine introuvable');
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
