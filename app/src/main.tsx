import React from 'react';
import ReactDOM from 'react-dom/client';
import { App } from './App';
import { loadDictionary } from './i18n';
import { useUi } from './stores/ui';
import { armAudioUnlock } from './lib/audio';
import { installerPiegesGlobaux } from './lib/journal';
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

// Erreurs et rejets de promesse non traités vers le journal du nœud (§10.6).
// Posé AVANT le rendu : ce qui casse au montage est exactement ce qu'on
// cherche à lire ensuite, et la console d'une webview de production n'existe
// pour personne.
installerPiegesGlobaux();

const root = document.getElementById('root');
if (root === null) {
  throw new Error('élément racine introuvable');
}

// Seul le français est dans le socle : la langue persistée est chargée avant
// le premier rendu, sinon l'application s'afficherait un instant en français
// puis basculerait. Une seule requête, et seulement pour les non-francophones.
//
// L'échec est rattrapé : un chunk introuvable (cache corrompu, fichier manquant
// après une mise à jour partielle) ne doit pas empêcher l'application de
// démarrer. L'interface s'affiche alors en français — dégradé, mais utilisable,
// et l'utilisateur peut rechoisir sa langue.
const lang = useUi.getState().lang;
if (lang !== 'fr') {
  try {
    await loadDictionary(lang);
  } catch {
    useUi.setState({ lang: 'fr' });
  }
}

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
