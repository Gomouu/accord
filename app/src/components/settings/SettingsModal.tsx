/**
 * Paramètres en plein écran, façon Discord : colonne de catégories à gauche,
 * contenu à droite, fermeture par Échap ou bouton dédié. Déclenché par
 * `ui.modal = { kind: 'settings' }` comme l'ancienne modale.
 */

import { useEffect, useRef, useState } from 'react';
import { useUi, useT } from '../../stores/ui';
import { DEFAULT_TAB, findTab, SETTINGS_GROUPS, type SettingsTabId } from './tabs';

export function SettingsModal() {
  const t = useT();
  const closeModal = useUi((s) => s.closeModal);
  const [tabId, setTabId] = useState<SettingsTabId>(DEFAULT_TAB.id);
  const navRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') closeModal();
    };
    window.addEventListener('keydown', onKey);
    // Le clavier démarre sur la catégorie active.
    navRef.current?.querySelector<HTMLButtonElement>('[aria-current="page"]')?.focus();
    return () => window.removeEventListener('keydown', onKey);
  }, [closeModal]);

  const active = findTab(tabId);
  const Content = active.Content;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={t.settings.title}
      className="modal-overlay-enter fixed inset-0 z-40 flex bg-chat"
    >
      <nav
        ref={navRef}
        aria-label={t.settings.title}
        className="flex w-1/3 min-w-[232px] shrink-0 justify-end overflow-y-auto bg-sidebar pb-10 pl-4 pr-2 pt-14"
      >
        <div className="w-[212px]">
          {SETTINGS_GROUPS.map((group, index) => (
            <div key={group.id}>
              {index > 0 && (
                <div className="mx-2.5 my-2 h-px bg-input" role="separator" />
              )}
              <div className="px-2.5 pb-1.5 text-xs font-bold uppercase tracking-wide text-faint">
                {group.label(t)}
              </div>
              {group.tabs.map((tab) => (
                <button
                  key={tab.id}
                  type="button"
                  aria-current={tabId === tab.id ? 'page' : undefined}
                  onClick={() => setTabId(tab.id)}
                  className={`mb-0.5 block w-full rounded px-2.5 py-1.5 text-left font-medium transition-colors duration-150 ${
                    tabId === tab.id
                      ? 'bg-chat-hover text-header'
                      : 'text-muted hover:bg-chat-hover hover:text-norm'
                  }`}
                >
                  {tab.label(t)}
                </button>
              ))}
            </div>
          ))}
        </div>
      </nav>

      <div className="flex min-w-0 flex-1">
        <section
          aria-label={active.label(t)}
          className="min-w-0 max-w-[740px] flex-1 overflow-y-auto px-10 pb-20 pt-14"
        >
          <h2 className="mb-6 text-xl font-bold text-header">{active.label(t)}</h2>
          <Content />
        </section>
        <div className="w-[84px] shrink-0 pt-14">
          <button
            type="button"
            aria-label={t.app.close}
            onClick={closeModal}
            className="group flex flex-col items-center"
          >
            <span className="flex h-9 w-9 items-center justify-center rounded-full border-2 border-faint text-faint transition-colors duration-150 group-hover:border-norm group-hover:text-norm">
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
              >
                <path d="M6.3 5 12 10.6 17.7 5 19 6.3 13.4 12l5.6 5.7-1.3 1.3-5.7-5.6L6.3 19 5 17.7l5.6-5.7L5 6.3 6.3 5Z" />
              </svg>
            </span>
            <span className="mt-1.5 text-xs font-semibold uppercase text-faint">
              {t.settings.escKey}
            </span>
          </button>
        </div>
      </div>
    </div>
  );
}
