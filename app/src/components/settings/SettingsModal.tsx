/**
 * Paramètres façon Discord moderne : fond assombri + flouté par-dessus
 * l'app, panneau flottant centré (colonne de catégories à gauche, contenu
 * à droite), fermeture par clic sur le fond, Échap ou bouton dédié.
 * Déclenché par `ui.modal = { kind: 'settings' }` comme l'ancienne modale.
 */

import { useEffect, useRef, useState } from 'react';
import { interpolate } from '../../i18n';
import { bouclerTab, deplacerFocusVertical } from '../../lib/focus';
import { useUi, useT } from '../../stores/ui';
import { CloseIcon } from '../ContextMenu';
import {
  DEFAULT_TAB,
  filterSettingsGroups,
  findTab,
  SETTINGS_GROUPS,
  type SettingsTabId,
} from './tabs';

export function SettingsModal() {
  const t = useT();
  const closeModal = useUi((s) => s.closeModal);
  const [tabId, setTabId] = useState<SettingsTabId>(DEFAULT_TAB.id);
  const [query, setQuery] = useState('');
  const groups = filterSettingsGroups(SETTINGS_GROUPS, t, query);
  const navRef = useRef<HTMLElement>(null);
  const dialogRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') closeModal();
    };
    window.addEventListener('keydown', onKey);
    // Le clavier démarre sur la catégorie active, puis revient au déclencheur
    // (bouton d'ouverture des réglages) à la fermeture.
    const declencheur =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    navRef.current?.querySelector<HTMLButtonElement>('[aria-current="page"]')?.focus();
    return () => {
      window.removeEventListener('keydown', onKey);
      if (declencheur !== null && declencheur.isConnected) declencheur.focus();
    };
  }, [closeModal]);

  const active = findTab(tabId);
  const Content = active.Content;

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-40 flex items-center justify-center bg-black/75 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeModal();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={t.settings.title}
        onKeyDown={(e) => bouclerTab(e, dialogRef.current)}
        className="modal-panel-enter accord-settings relative flex h-[94vh] w-[min(1100px,94vw)] overflow-hidden rounded-xl bg-chat shadow-3 max-sm:h-full max-sm:w-full max-sm:rounded-none"
      >
        <nav
          ref={navRef}
          aria-label={t.settings.title}
          onKeyDown={(e) => deplacerFocusVertical(e, navRef.current)}
          className="accord-settings-nav flex w-[30%] min-w-[180px] shrink-0 justify-end overflow-y-auto border-r border-rail/60 bg-sidebar pb-8 pl-3 pr-2 pt-12 max-sm:w-[132px] max-sm:min-w-[132px] max-sm:pl-2 max-sm:pt-14"
        >
          <div className="w-[212px] max-sm:w-full">
            <input
              type="search"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              aria-label={t.settings.searchPlaceholder}
              placeholder={t.settings.searchPlaceholder}
              className="mb-2 min-h-9 w-full rounded-md border border-transparent bg-input px-2.5 py-1.5 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
            />
            {groups.map((group, index) => (
              <div key={group.id}>
                {index > 0 && (
                  <div className="mx-2.5 my-2 h-px bg-input/60" role="separator" />
                )}
                <div className="px-2.5 pb-1.5 text-xs font-medium uppercase tracking-wide text-faint">
                  {group.label(t)}
                </div>
                {group.tabs.map((tab) => (
                  <button
                    key={tab.id}
                    type="button"
                    aria-current={tabId === tab.id ? 'page' : undefined}
                    onClick={() => setTabId(tab.id)}
                    className={`mb-0.5 block min-h-9 w-full truncate rounded-md px-2.5 py-1.5 text-left text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
                      tabId === tab.id
                        ? 'bg-blurple/15 text-header ring-1 ring-inset ring-blurple/20'
                        : 'text-muted hover:bg-chat-hover hover:text-norm'
                    }`}
                  >
                    {tab.label(t)}
                  </button>
                ))}
              </div>
            ))}
            {groups.length === 0 && (
              <p className="px-2.5 py-6 text-center text-sm text-muted">
                {interpolate(t.settings.searchNoResults, { query: query.trim() })}
              </p>
            )}
          </div>
        </nav>

        <div className="flex min-w-0 flex-1">
          <section
            aria-label={active.label(t)}
            className="accord-settings-content min-w-0 max-w-[740px] flex-1 overflow-y-auto px-6 pb-20 pt-14 max-sm:px-4"
          >
            <h2 className="mb-6 text-xl font-semibold text-header">{active.label(t)}</h2>
            <Content />
          </section>
          <div className="absolute right-3 top-3">
            <button
              type="button"
              aria-label={t.app.close}
              onClick={closeModal}
              className="group flex flex-col items-center focus-visible:outline-none"
            >
              <span className="flex h-10 w-10 items-center justify-center rounded-full border-2 border-faint text-faint transition-colors duration-fast group-hover:border-norm group-hover:text-norm group-focus-visible:ring-2 group-focus-visible:ring-blurple group-focus-visible:ring-offset-2 group-focus-visible:ring-offset-chat">
                <CloseIcon size={18} />
              </span>
              <span className="mt-1.5 text-xs font-medium uppercase text-faint max-sm:hidden">
                {t.settings.escKey}
              </span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
