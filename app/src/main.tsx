import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import { loadDictionary } from './i18n';
import { useUi } from './stores/ui';
import { armAudioUnlock } from './lib/audio';
import './styles/global.css';
import './styles/theme-scenes.css';
import './styles/figurative-themes.css';
import './styles/profile-personalization.css';
import './styles/profile-surfaces.css';
import './styles/identity-refresh.css';
import './styles/liquid-glass.css';
import './styles/theme-coverage.css';

// Déverrouillage audio armé avant tout : le premier geste (clic, frappe —
// onboarding compris) met le contexte Web Audio partagé en route, pour que
// blip, sonnerie et soundboard soient audibles dès le premier événement.
armAudioUnlock();

const root = document.getElementById('root');
if (root === null) {
  throw new Error('élément racine introuvable');
}

// Seul le français est dans le socle : la langue persistée est chargée avant
// le premier rendu, sinon l'application s'afficherait un instant en français
// puis basculerait. Une seule requête, et seulement pour les non-francophones.
const lang = useUi.getState().lang;
if (lang !== 'fr') {
  await loadDictionary(lang);
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
