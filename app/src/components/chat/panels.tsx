/**
 * Volets du fil de conversation, extraits de `ChatView` (D-056) : messages
 * épinglés, liste des fils du salon, et barre du mode sélection (suppression
 * groupée). Comportement inchangé.
 */

import { useEffect, useRef, useState } from 'react';
import { interpolate } from '../../i18n';
import type { GroupThread } from '../../lib/api';
import { formatTimestamp } from '../../lib/format';
import { useUi, useT } from '../../stores/ui';
import { AttachmentRow } from '../Attachments';
import { CloseIcon } from '../ContextMenu';
import type { DisplayMessage } from '../MessageList';

/** Borne de la suppression groupée (messages sélectionnés d'un coup). */
export const PURGE_MAX = 100;

/**
 * Volet des messages épinglés (MP ou salon) : messages déjà résolus dans
 * l'historique chargé + nombre d'épinglés hors-page. Chaque entrée saute vers
 * le message d'un clic ; `canUnpin` expose le retrait de l'épingle.
 */
export function PinnedPanel({
  resolved,
  unresolved,
  canUnpin,
  onUnpin,
  onJump,
  onClose,
  nameOf,
}: {
  resolved: readonly DisplayMessage[];
  unresolved: number;
  canUnpin: boolean;
  onUnpin: (msgId: string) => void;
  onJump: (msgId: string) => void;
  onClose: () => void;
  nameOf: (author: string) => string;
}) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const timeFormat = useUi((s) => s.timeFormat);
  const panelRef = useRef<HTMLDivElement>(null);

  // Échap referme le volet ; le focus entre dans le volet à l'ouverture et
  // revient au déclencheur (bouton « Épinglés ») à la fermeture.
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    const declencheur =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    panelRef.current?.focus();
    return () => {
      window.removeEventListener('keydown', onKey);
      if (declencheur !== null && declencheur.isConnected) declencheur.focus();
    };
  }, [onClose]);

  return (
    <div
      ref={panelRef}
      role="dialog"
      aria-label={t.serveur.pinnedTitle}
      tabIndex={-1}
      className="glass-strong liquid-floating absolute right-4 top-14 z-20 max-h-96 w-96 max-w-[85vw] overflow-y-auto rounded-lg p-3 focus:outline-none"
    >
      <div className="flex items-center justify-between pb-2">
        <span className="text-sm font-semibold text-header">{t.serveur.pinnedTitle}</span>
        <button
          type="button"
          aria-label={t.app.close}
          onClick={onClose}
          className="rounded-full p-1 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
        >
          <CloseIcon size={16} />
        </button>
      </div>
      {resolved.length === 0 && unresolved === 0 && (
        <p className="py-3 text-center text-sm text-muted">{t.serveur.noPins}</p>
      )}
      {resolved.map((m) => (
        <div key={m.msg_id} className="group/pin mb-1 rounded-md bg-sidebar px-3 py-2">
          <button
            type="button"
            onClick={() => onJump(m.msg_id)}
            className="block w-full rounded-sm text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
          >
            <div className="flex items-baseline justify-between gap-2">
              <span className="truncate text-sm font-medium text-header">
                {nameOf(m.author)}
              </span>
              <span className="shrink-0 text-xs text-faint">
                {formatTimestamp(m.sent_ms, lang, undefined, timeFormat)}
              </span>
            </div>
            {(m.edited ?? (m.body.type === 'text' ? m.body.text : '')) !== '' && (
              <div className="break-words text-sm text-norm">
                {m.edited ?? (m.body.type === 'text' ? m.body.text : '')}
              </div>
            )}
            {m.body.type === 'sticker' && (
              <div className="text-sm italic text-muted">{t.serveur.pinSticker}</div>
            )}
            {m.body.type === 'poll' && (
              <div className="text-sm italic text-muted">{t.serveur.pinPoll}</div>
            )}
          </button>
          {(m.attachments?.length ?? 0) > 0 && (
            <div className="mt-1">
              <AttachmentRow pieces={m.attachments ?? []} hint={m.author} />
            </div>
          )}
          {canUnpin && (
            <button
              type="button"
              onClick={() => onUnpin(m.msg_id)}
              className="mt-1 rounded-sm text-xs font-medium text-muted transition-colors duration-fast hover:text-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
            >
              {t.serveur.unpin}
            </button>
          )}
        </div>
      ))}
      {unresolved > 0 && (
        <p className="pt-1 text-center text-xs text-faint">
          {interpolate(t.serveur.pinsNotLoaded, { count: String(unresolved) })}
        </p>
      )}
    </div>
  );
}

/**
 * Popover listant les fils du salon courant (actifs puis archivés). Un clic
 * ouvre le panneau du fil. Calqué visuellement sur `PinnedPanel`.
 */
export function ThreadsListPanel({
  threads,
  onOpen,
  onClose,
}: {
  threads: readonly GroupThread[];
  onOpen: (threadId: string) => void;
  onClose: () => void;
}) {
  const t = useT();
  const active = threads.filter((th) => !th.archived);
  const archived = threads.filter((th) => th.archived);

  const row = (th: GroupThread) => (
    <button
      key={th.thread_id}
      type="button"
      onClick={() => onOpen(th.thread_id)}
      className="mb-1 flex w-full items-center gap-2 rounded-md bg-sidebar px-3 py-2 text-left transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
    >
      <span aria-hidden className="shrink-0 text-sm leading-none">
        💬
      </span>
      <span className="min-w-0 flex-1 truncate text-sm font-medium text-header">
        {th.name}
      </span>
    </button>
  );

  return (
    <div
      role="menu"
      aria-label={t.threads.threadsList}
      className="glass-strong liquid-floating absolute right-4 top-14 z-20 max-h-96 w-80 max-w-[85vw] overflow-y-auto rounded-lg p-3"
    >
      <div className="flex items-center justify-between pb-2">
        <span className="text-sm font-semibold text-header">{t.threads.threadsList}</span>
        <button
          type="button"
          aria-label={t.app.close}
          onClick={onClose}
          className="rounded-full p-1 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
        >
          <CloseIcon size={16} />
        </button>
      </div>
      {threads.length === 0 && (
        <p className="py-3 text-center text-sm text-muted">{t.threads.empty}</p>
      )}
      {active.length > 0 && (
        <>
          <p className="px-1 pb-1 text-[11px] font-semibold uppercase tracking-wide text-faint">
            {t.threads.active}
          </p>
          {active.map(row)}
        </>
      )}
      {archived.length > 0 && (
        <>
          <p className="px-1 pb-1 pt-2 text-[11px] font-semibold uppercase tracking-wide text-faint">
            {t.threads.archived}
          </p>
          {archived.map(row)}
        </>
      )}
    </div>
  );
}

/**
 * Barre d'action du mode sélection (suppression groupée) : compteur, garde de
 * borne (100), suppression en deux temps (confirmation en place) et sortie du
 * mode. Rendue en tête du salon tant que le mode est actif (voir `GroupView`).
 */
export function SelectionBar({
  count,
  onDelete,
  onCancel,
}: {
  count: number;
  onDelete: () => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [confirming, setConfirming] = useState(false);
  const tooMany = count > PURGE_MAX;
  const canDelete = count > 0 && !tooMany;
  return (
    <div
      role="toolbar"
      aria-label={t.purge.select}
      className="flex h-11 shrink-0 items-center gap-3 border-b border-[color:var(--glass-border)] bg-chat/90 px-4 shadow-1"
    >
      <span className="text-sm font-medium text-norm">
        {interpolate(t.purge.selected, { count: String(count) })}
      </span>
      {tooMany && <span className="text-xs font-medium text-red">{t.purge.tooMany}</span>}
      <div className="ml-auto flex items-center gap-2">
        {confirming ? (
          <>
            <span className="text-sm text-norm">
              {interpolate(t.purge.confirmTitle, { count: String(count) })}
            </span>
            <button
              type="button"
              onClick={() => {
                setConfirming(false);
                onDelete();
              }}
              className="rounded-sm bg-red px-2.5 py-1 text-xs font-medium text-on-red transition-colors hover:bg-red/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
            >
              {t.purge.confirm}
            </button>
            <button
              type="button"
              onClick={() => setConfirming(false)}
              className="rounded-sm px-2 py-1 text-xs font-medium text-norm transition-colors hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
            >
              {t.purge.cancel}
            </button>
          </>
        ) : (
          <>
            <button
              type="button"
              disabled={!canDelete}
              onClick={() => setConfirming(true)}
              className="rounded-sm bg-red px-2.5 py-1 text-xs font-medium text-on-red transition-colors hover:bg-red/80 disabled:cursor-not-allowed disabled:opacity-50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
            >
              {t.purge.delete}
            </button>
            <button
              type="button"
              onClick={onCancel}
              className="rounded-sm px-2 py-1 text-xs font-medium text-norm transition-colors hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
            >
              {t.purge.cancel}
            </button>
          </>
        )}
      </div>
    </div>
  );
}
