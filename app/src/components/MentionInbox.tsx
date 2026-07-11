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
    void useFriends.getState().load().catch(() => {});
    void useGroups.getState().refreshUnread().catch(() => {});
  };

  const openEntry = (entry: MentionEntry): void => {
    void api.mentionsMarkRead([entry.msg_id]).then(refreshBadges).catch(() => {});
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
        toast('info', t.mentions.allRead);
      })
      .catch(() => toast('error', t.errors.actionFailed));
  };

  const hasUnread = entries?.some((e) => !e.read) ?? false;

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-16"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label={t.mentions.inboxTitle}
        onClick={(e) => e.stopPropagation()}
        className="modal-panel-enter max-h-[70vh] w-[26rem] max-w-[90vw] overflow-hidden rounded-lg border border-rail bg-modal shadow-modal"
      >
        <div className="flex items-center justify-between border-b border-rail px-4 py-3">
          <span className="text-sm font-semibold text-header">
            {t.mentions.inboxTitle}
          </span>
          <div className="flex items-center gap-2">
            {hasUnread && (
              <button
                type="button"
                onClick={markAllRead}
                className="rounded px-2 py-1 text-xs font-medium text-muted transition-colors duration-fast hover:text-norm"
              >
                {t.mentions.markAllRead}
              </button>
            )}
            <button
              type="button"
              aria-label={t.app.close}
              onClick={onClose}
              className="rounded p-1 text-faint transition-colors duration-fast hover:text-norm active:scale-95"
            >
              <svg
                width="16"
                height="16"
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
              >
                <path d="M6.3 5 12 10.6 17.7 5 19 6.3 13.4 12l5.6 5.7-1.3 1.3-5.7-5.6L6.3 19 5 17.7l5.6-5.7L5 6.3 6.3 5Z" />
              </svg>
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
              className="mb-1 block w-full rounded px-3 py-2 text-left transition-colors duration-fast hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none"
            >
              <div className="flex items-baseline gap-2">
                {!entry.read && (
                  <span
                    aria-hidden
                    className="h-2 w-2 shrink-0 rounded-full bg-red"
                  />
                )}
                <span className="min-w-0 flex-1 truncate text-sm font-medium text-header">
                  {displayNameOf(contacts, entry.author)}
                </span>
                <span className="shrink-0 text-xs text-faint">
                  {formatTimestamp(entry.ts_ms, lang)}
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
