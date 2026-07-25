/**
 * Panneau des messages enregistrés (favoris locaux) : liste, saut au message
 * et retrait. Miroir visuel de `MentionInbox` (overlay centré), alimenté par le
 * store local `useSaved` — aucun appel réseau, rien ne quitte l'appareil.
 */

import { useEffect } from 'react';
import { formatTimestamp } from '../lib/format';
import { displayNameOf, useFriends } from '../stores/friends';
import { useSaved } from '../stores/saved';
import { useUi, useT } from '../stores/ui';
import { CloseIcon } from './ContextMenu';
import { EmptyState } from './EmptyState';
import { BookmarkMenuIcon } from './messageMenus';

export function SavedMessages({ onClose }: { onClose: () => void }) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const timeFormat = useUi((s) => s.timeFormat);
  const requestJump = useUi((s) => s.requestJump);
  const setView = useUi((s) => s.setView);
  const contacts = useFriends((s) => s.contacts);
  const items = useSaved((s) => s.items);
  const remove = useSaved((s) => s.remove);
  const clear = useSaved((s) => s.clear);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  const open = (msgId: string, view: (typeof items)[number]['view']): void => {
    // Un salon sans channel ne peut qu'ouvrir le groupe, pas sauter au message.
    if (view.kind === 'group' && view.channelId === null) setView(view);
    else requestJump(view, msgId);
    onClose();
  };

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-50 flex items-start justify-center bg-black/60 pt-16 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label={t.saved.title}
        onClick={(e) => e.stopPropagation()}
        className="glass modal-panel-enter max-h-[70vh] w-[26rem] max-w-[90vw] overflow-hidden rounded-xl shadow-3"
      >
        <div className="flex items-center justify-between border-b border-input/50 px-4 py-3">
          <span className="text-sm font-semibold text-header">{t.saved.title}</span>
          <div className="flex items-center gap-2">
            {items.length > 0 && (
              <button
                type="button"
                onClick={clear}
                className="rounded-sm px-2 py-1 text-xs font-medium text-muted transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
              >
                {t.saved.clearAll}
              </button>
            )}
            <button
              type="button"
              aria-label={t.app.close}
              onClick={onClose}
              className="rounded-sm p-1 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
            >
              <CloseIcon size={16} />
            </button>
          </div>
        </div>
        <div className="max-h-[calc(70vh-3.25rem)] overflow-y-auto p-2">
          {items.length === 0 ? (
            <EmptyState compact label={t.saved.empty} icon={<BookmarkMenuIcon />} />
          ) : (
            items.map((item) => (
              <div
                key={item.msgId}
                className="group mb-1 flex items-start gap-1 rounded-md pe-1 transition-colors duration-fast hover:bg-chat-hover"
              >
                <button
                  type="button"
                  aria-label={t.saved.open}
                  onClick={() => open(item.msgId, item.view)}
                  className="min-w-0 flex-1 rounded-md px-3 py-2 text-start focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
                >
                  <div className="flex items-baseline gap-2">
                    <span className="min-w-0 flex-1 truncate text-sm font-medium text-header">
                      {displayNameOf(contacts, item.author)}
                    </span>
                    <span className="shrink-0 text-xs text-faint">
                      {formatTimestamp(item.ts, lang, undefined, timeFormat)}
                    </span>
                  </div>
                  <div className="mt-0.5 break-words text-sm text-muted">
                    {item.text === '' ? '—' : item.text}
                  </div>
                </button>
                <button
                  type="button"
                  aria-label={t.saved.remove}
                  onClick={() => remove(item.msgId)}
                  className="mt-2 shrink-0 rounded-sm p-1 text-faint opacity-0 transition-opacity duration-fast hover:text-norm focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple group-hover:opacity-100"
                >
                  <CloseIcon size={14} />
                </button>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}
