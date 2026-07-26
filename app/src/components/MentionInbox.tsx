/**
 * Mention inbox: a Discord-style panel listing the most recent messages that
 * mention the local user (`mentions.inbox`). Clicking an entry marks it read
 * and jumps to the message (reusing the wave-2a jump mechanism); a group entry
 * without a channel falls back to opening the group. A single action marks
 * every mention as read. The panel opens as a centred overlay.
 */

import { useEffect, useState } from 'react';
import type { MentionConversation, MentionEntry } from '../lib/api';
import { api } from '../lib/client';
import { formatTimestamp } from '../lib/format';
import { displayNameOf, useFriends } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { useUi, useT, type View } from '../stores/ui';
import { CloseIcon } from './ContextMenu';

/** Maps an inbox conversation reference onto a navigable UI view. */
function viewOf(conversation: MentionConversation): View {
  if (conversation.kind === 'dm') return { kind: 'dm', peer: conversation.peer };
  return {
    kind: 'group',
    groupId: conversation.group_id,
    channelId: conversation.channel_id,
  };
}

export function MentionInbox({ onClose }: { onClose: () => void }) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const timeFormat = useUi((s) => s.timeFormat);
  const requestJump = useUi((s) => s.requestJump);
  const setView = useUi((s) => s.setView);
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const [entries, setEntries] = useState<MentionEntry[] | null>(null);

  useEffect(() => {
    let alive = true;
    api
      .mentionsInbox()
      .then((res) => {
        if (alive) setEntries(res.entries);
      })
      .catch(() => {
        if (alive) setEntries([]);
      });
    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [onClose]);

  /** Refreshes the badge counters after any read-state change. */
  const refreshBadges = (): void => {
    void useFriends
      .getState()
      .load()
      .catch(() => {
        // Best effort : les compteurs seront corrigés au prochain passage.
      });
    void useGroups
      .getState()
      .refreshUnread()
      .catch(() => {
        // Best effort : les compteurs seront corrigés au prochain passage.
      });
  };

  const openEntry = (entry: MentionEntry): void => {
    void api
      .mentionsMarkRead([entry.msg_id])
      .then(refreshBadges)
      .catch(() => toast('error', t.errors.actionFailed));
    const view = viewOf(entry.conversation);
    // A group mention without a channel can only open the group, not jump.
    if (view.kind === 'group' && view.channelId === null) setView(view);
    else requestJump(view, entry.msg_id);
    onClose();
  };

  const markAllRead = (): void => {
    void api
      .mentionsMarkRead()
      .then(() => {
        refreshBadges();
        setEntries((prev) => prev?.map((e) => ({ ...e, read: true })) ?? prev);
        toast('success', t.mentions.allRead);
      })
      .catch(() => toast('error', t.errors.actionFailed));
  };

  const hasUnread = entries?.some((e) => !e.read) ?? false;

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-50 flex items-start justify-center bg-black/60 pt-16 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label={t.mentions.inboxTitle}
        onClick={(e) => e.stopPropagation()}
        className="glass modal-panel-enter max-h-[70vh] w-[26rem] max-w-[90vw] overflow-hidden rounded-xl shadow-3"
      >
        <div className="flex items-center justify-between border-b border-input/50 px-4 py-3">
          <span className="text-sm font-semibold text-header">
            {t.mentions.inboxTitle}
          </span>
          <div className="flex items-center gap-2">
            {hasUnread && (
              <button
                type="button"
                onClick={markAllRead}
                className="rounded-sm px-2 py-1 text-xs font-medium text-muted transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
              >
                {t.mentions.markAllRead}
              </button>
            )}
            <button
              type="button"
              aria-label={t.app.close}
              onClick={onClose}
              // 24 → 40 px. Le débordement vers le début s'arrête pile sur
              // l'écart de 8 px qui sépare la croix de « Tout marquer lu » :
              // les deux cibles se touchent sans se recouvrir.
              className="relative -m-2 rounded-sm p-3 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
            >
              <CloseIcon size={16} />
            </button>
          </div>
        </div>
        <div className="max-h-[calc(70vh-3.25rem)] overflow-y-auto p-2">
          {entries === null && (
            <p className="py-6 text-center text-sm text-muted">{t.mentions.loading}</p>
          )}
          {entries !== null && entries.length === 0 && (
            <p className="py-6 text-center text-sm text-muted">{t.mentions.empty}</p>
          )}
          {entries?.map((entry) => (
            <button
              key={entry.msg_id}
              type="button"
              onClick={() => openEntry(entry)}
              className="mb-1 block w-full rounded-md px-3 py-2 text-start transition-colors duration-fast hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none"
            >
              <div className="flex items-baseline gap-2">
                {!entry.read && (
                  <span aria-hidden className="h-2 w-2 shrink-0 rounded-full bg-red" />
                )}
                <span className="min-w-0 flex-1 truncate text-sm font-medium text-header">
                  {displayNameOf(contacts, entry.author)}
                </span>
                <span className="shrink-0 text-xs text-faint">
                  {formatTimestamp(entry.ts_ms, lang, undefined, timeFormat)}
                </span>
              </div>
              <div className="mt-0.5 break-words text-sm text-muted">{entry.snippet}</div>
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
