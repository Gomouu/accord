/**
 * Forward picker: a small centred dialog that re-sends a message's text and
 * attachment references to another conversation. It lists the user's friends
 * (direct messages) and every text/announcement channel of the groups they
 * belong to. Voice channels are excluded (no message stream). Self-contained:
 * it reads the friends/groups stores and drives their `send` actions directly.
 */

import { useState } from 'react';
import type { FileAttachment } from '../lib/api';
import { displayNameOf, useFriends } from '../stores/friends';
import { sortChannels, useGroups } from '../stores/groups';
import { useDms } from '../stores/dms';
import { useUi, useT } from '../stores/ui';

/** A resolved forward destination and the send it performs. */
interface ForwardTarget {
  key: string;
  label: string;
  send: (text: string, attachments?: FileAttachment[]) => Promise<void>;
}

interface ForwardPickerProps {
  text: string;
  attachments?: FileAttachment[] | undefined;
  onClose: () => void;
}

export function ForwardPicker({ text, attachments, onClose }: ForwardPickerProps) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const groupIds = useGroups((s) => s.ids);
  const groupStates = useGroups((s) => s.states);
  const [busy, setBusy] = useState(false);

  const dmSend = useDms((s) => s.send);
  const groupSend = useGroups((s) => s.send);

  const dmTargets: ForwardTarget[] = contacts
    .filter((c) => c.state === 'friend')
    .map((c) => ({
      key: `dm:${c.pubkey}`,
      label: `@${displayNameOf(contacts, c.pubkey)}`,
      send: (msg, files) => dmSend(c.pubkey, msg, undefined, files),
    }));

  const groupTargets: { name: string; channels: ForwardTarget[] }[] = groupIds.flatMap(
    (groupId) => {
      const state = groupStates[groupId];
      if (state === undefined) return [];
      const channels = sortChannels(state.channels)
        .filter((ch) => ch.kind !== 'voice')
        .map((ch) => ({
          key: `group:${groupId}:${ch.channel_id}`,
          label: `#${ch.name}`,
          send: (msg: string, files?: FileAttachment[]) =>
            groupSend(groupId, ch.channel_id, msg, undefined, files),
        }));
      return channels.length > 0 ? [{ name: state.name, channels }] : [];
    },
  );

  const isEmpty = dmTargets.length === 0 && groupTargets.length === 0;

  const forwardTo = (target: ForwardTarget): void => {
    if (busy) return;
    setBusy(true);
    void target
      .send(text, attachments && attachments.length > 0 ? attachments : undefined)
      .then(() => {
        toast('info', t.dm.forwarded);
        onClose();
      })
      .catch(() => {
        toast('error', t.dm.forwardFailed);
        setBusy(false);
      });
  };

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-16"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-label={t.dm.forwardTitle}
        onClick={(e) => e.stopPropagation()}
        className="modal-panel-enter max-h-[70vh] w-[24rem] max-w-[90vw] overflow-hidden rounded-lg border border-rail bg-modal shadow-modal"
      >
        <div className="flex items-center justify-between border-b border-rail px-4 py-3">
          <span className="text-sm font-semibold text-header">{t.dm.forwardTitle}</span>
          <button
            type="button"
            aria-label={t.app.close}
            onClick={onClose}
            className="rounded p-1 text-faint transition-colors duration-fast hover:text-norm active:scale-95"
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
              <path d="M6.3 5 12 10.6 17.7 5 19 6.3 13.4 12l5.6 5.7-1.3 1.3-5.7-5.6L6.3 19 5 17.7l5.6-5.7L5 6.3 6.3 5Z" />
            </svg>
          </button>
        </div>
        <div className="max-h-[calc(70vh-3.25rem)] overflow-y-auto p-2">
          {isEmpty && (
            <p className="py-6 text-center text-sm text-muted">{t.dm.forwardEmpty}</p>
          )}
          {dmTargets.length > 0 && (
            <div className="px-2 pb-1 pt-1 text-xs font-semibold uppercase tracking-wide text-faint">
              {t.dm.directMessages}
            </div>
          )}
          {dmTargets.map((target) => (
            <button
              key={target.key}
              type="button"
              disabled={busy}
              onClick={() => forwardTo(target)}
              className="block w-full truncate rounded px-3 py-1.5 text-left text-sm text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm disabled:opacity-40"
            >
              {target.label}
            </button>
          ))}
          {groupTargets.map((group) => (
            <div key={group.name}>
              <div className="truncate px-2 pb-1 pt-3 text-xs font-semibold uppercase tracking-wide text-faint">
                {group.name}
              </div>
              {group.channels.map((target) => (
                <button
                  key={target.key}
                  type="button"
                  disabled={busy}
                  onClick={() => forwardTo(target)}
                  className="block w-full truncate rounded px-3 py-1.5 text-left text-sm text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm disabled:opacity-40"
                >
                  {target.label}
                </button>
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
