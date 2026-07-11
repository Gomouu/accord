/**
 * Paramètres façon Discord moderne : fond assombri + flouté par-dessus
 * l'app, panneau flottant centré (colonne de catégories à gauche, contenu
 * à droite), fermeture par clic sur le fond, Échap ou bouton dédié.
 * Déclenché par `ui.modal = { kind: 'settings' }` comme l'ancienne modale.
 */

import { useEffect, useRef, useState } from 'react';
import { useUi, useT } from '../../stores/ui';
import { CloseIcon } from '../ContextMenu';
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
      className="modal-overlay-enter fixed inset-0 z-40 flex items-center justify-center bg-black/75 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeModal();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t.settings.title}
        className="modal-panel-enter relative flex h-[94vh] w-[min(1100px,94vw)] overflow-hidden rounded-xl bg-chat shadow-3"
      >
        <nav
          ref={navRef}
          aria-label={t.settings.title}
          className="flex w-1/3 min-w-[232px] shrink-0 justify-end overflow-y-auto border-r border-rail/60 bg-sidebar pb-10 pl-4 pr-2 pt-14"
        >
          <div className="w-[212px]">
            {SETTINGS_GROUPS.map((group, index) => (
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
                    className={`mb-0.5 block w-full rounded-md px-2.5 py-1.5 text-left font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
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
            <h2 className="mb-6 text-xl font-semibold text-header">{active.label(t)}</h2>
            <Content />
          </section>
          <div className="w-[84px] shrink-0 pt-14">
            <button
              type="button"
              aria-label={t.app.close}
              onClick={closeModal}
              className="group flex flex-col items-center focus-visible:outline-none"
            >
              <span className="flex h-9 w-9 items-center justify-center rounded-full border-2 border-faint text-faint transition-colors duration-fast group-hover:border-norm group-hover:text-norm group-focus-visible:ring-2 group-focus-visible:ring-blurple group-focus-visible:ring-offset-2 group-focus-visible:ring-offset-chat">
                <CloseIcon size={18} />
              </span>
              <span className="mt-1.5 text-xs font-medium uppercase text-faint">
                {t.settings.escKey}
              </span>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
