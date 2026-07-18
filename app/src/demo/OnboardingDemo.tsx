import React from 'react';
import ReactDOM from 'react-dom/client';
import { Toasts } from '../components/Toasts';
import { Onboarding } from '../screens/Onboarding';
import { useSession } from '../stores/session';
import { useUi } from '../stores/ui';
import '../styles/global.css';
import '../styles/theme-scenes.css';
import '../styles/profile-personalization.css';
import '../styles/profile-personalization-extra.css';
import '../styles/profile-personalization-more.css';
import '../styles/profile-surfaces.css';
import '../styles/identity-refresh.css';

const noop = async (): Promise<void> => {};

useSession.setState({
  phase: 'setup',
  self: null,
  askName: false,
  error: null,
  create: noop,
  restore: noop,
  goToWelcome: noop,
});
useUi.setState({ lang: 'fr', theme: 'dark', toasts: [] });
useUi.getState().setTheme('dark');

const root = document.getElementById('root');
if (root === null) throw new Error('élément racine introuvable');

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <Onboarding />
    <Toasts />
  </React.StrictMode>,
);
