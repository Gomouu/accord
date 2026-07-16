/**
 * Panneau latéral d'un fil de discussion (ouvert depuis un message ou la liste
 * des fils). Un fil se comporte comme un salon : ses messages voyagent par les
 * mêmes stores/RPC avec `thread_id` en guise de `channel_id`. Le panneau
 * réutilise donc `MessageList` + `MessageInput` à l'identique de la vue salon,
 * précédés d'un en-tête (nom, archivage gaté MANAGE_CHANNELS, fermeture) et du
 * message racine rappelé en contexte.
 */

import { useEffect, useState } from 'react';
import { interpolate } from '../i18n';
import type { GroupThread } from '../lib/api';
import { channelKey, useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { groupTypingKey } from '../stores/typing';
import { useT, useUi } from '../stores/ui';
import { CloseIcon } from './ContextMenu';
import { MessageInput } from './MessageInput';
import { MessageList, type DisplayMessage } from './MessageList';
import { TypingIndicator } from './TypingIndicator';
import { displayText } from './messageModel';

export interface ThreadPanelProps {
  groupId: string;
  thread: GroupThread;
  /** Message racine (contexte en tête) ; `undefined` s'il n'est pas chargé. */
  rootMessage: DisplayMessage | undefined;
  /** MANAGE_CHANNELS dans le salon parent : ouvre l'archivage/désarchivage. */
  canManage: boolean;
  /** MANAGE_MESSAGES : autorise la suppression des messages d'autrui. */
  canModerate: boolean;
  colorOf: (author: string) => string | null;
  emojiMap: ReadonlyMap<string, string>;
  knownMentions: ReadonlySet<string>;
  automodWords: readonly string[];
  nameOf: (author: string) => string;
  onClose: () => void;
}

export function ThreadPanel({
  groupId,
  thread,
  rootMessage,
  canManage,
  canModerate,
  colorOf,
  emojiMap,
  knownMentions,
  automodWords,
  nameOf,
  onClose,
}: ThreadPanelProps) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const self = useSession((s) => s.self);
  const threadId = thread.thread_id;
  const key = channelKey(groupId, threadId);
  const messages = useGroups((s) => s.messages[key]) ?? [];
  const hasMore = useGroups((s) => s.hasMore[key]) === true;
  const refreshHistory = useGroups((s) => s.refreshHistory);
  const loadOlderHistory = useGroups((s) => s.loadOlderHistory);
  const send = useGroups((s) => s.send);
  const editMessage = useGroups((s) => s.editMessage);
  const deleteMessage = useGroups((s) => s.deleteMessage);
  const toggleReaction = useGroups((s) => s.toggleReaction);
  const archiveThread = useGroups((s) => s.archiveThread);
  /** Message auquel la prochaine saisie répondra (null : envoi simple). */
  const [replyTo, setReplyTo] = useState<DisplayMessage | null>(null);

  // Historique du fil chargé (ou rechargé) à l'ouverture et au changement de
  // fil — même best effort que la vue salon.
  useEffect(() => {
    setReplyTo(null);
    refreshHistory(groupId, threadId).catch(() => toast('error', t.errors.loadFailed));
  }, [groupId, threadId, refreshHistory, toast, t]);

  const onActionError = (): void => toast('error', t.errors.actionFailed);
  const rootText = rootMessage !== undefined ? (displayText(rootMessage) ?? '') : '';

  return (
    <aside
      aria-label={interpolate(t.threads.panelLabel, { name: thread.name })}
      className="thread-panel flex shrink-0 flex-col border-l border-[color:var(--glass-border)] bg-chat/95"
    >
      <header className="flex h-12 shrink-0 items-center gap-2 border-b border-[color:var(--glass-border)] px-3 shadow-1">
        <span aria-hidden className="shrink-0 text-base leading-none">
          💬
        </span>
        <span
          className="min-w-0 flex-1 truncate font-semibold text-header"
          title={thread.name}
        >
          {thread.name}
        </span>
        {thread.archived && (
          <span className="shrink-0 rounded-full bg-chat-hover px-2 py-0.5 text-[11px] font-medium text-faint">
            {t.threads.archived}
          </span>
        )}
        {canManage && (
          <button
            type="button"
            onClick={() =>
              archiveThread(groupId, threadId, !thread.archived).catch(onActionError)
            }
            className="shrink-0 rounded-md px-2 py-1 text-xs font-medium text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat active:scale-95"
          >
            {thread.archived ? t.threads.unarchive : t.threads.archive}
          </button>
        )}
        <button
          type="button"
          aria-label={t.app.close}
          title={t.app.close}
          onClick={onClose}
          className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full text-faint transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat active:scale-95"
        >
          <CloseIcon size={16} />
        </button>
      </header>
      {rootMessage !== undefined && rootText !== '' && (
        <div className="mx-3 mt-3 rounded-md border-l-2 border-blurple bg-sidebar px-3 py-2">
          <div className="mb-0.5 flex items-baseline gap-2">
            <span
              className="truncate text-sm font-semibold text-header"
              style={
                colorOf(rootMessage.author) !== null
                  ? { color: colorOf(rootMessage.author) as string }
                  : undefined
              }
            >
              {nameOf(rootMessage.author)}
            </span>
            <span className="shrink-0 text-[11px] uppercase tracking-wide text-faint">
              {t.threads.rootMessage}
            </span>
          </div>
          <p className="line-clamp-3 break-words text-sm text-norm">{rootText}</p>
        </div>
      )}
      <MessageList
        key={key}
        messages={messages}
        hasMore={hasMore}
        onLoadOlder={() => {
          loadOlderHistory(groupId, threadId).catch(() =>
            toast('error', t.errors.loadFailed),
          );
        }}
        actions={{
          onReact: (message, emoji) => {
            if (!self) return;
            toggleReaction(groupId, threadId, message.msg_id, emoji, self.pubkey).catch(
              onActionError,
            );
          },
          onReply: (message) => setReplyTo(message),
          onEdit: (message, text) => {
            editMessage(groupId, threadId, message.msg_id, text).catch(onActionError);
          },
          onDelete: (message) => {
            deleteMessage(groupId, threadId, message.msg_id).catch(onActionError);
          },
          canModerate,
        }}
        colorOf={colorOf}
        emojiMap={emojiMap}
        knownMentions={knownMentions}
        automodWords={automodWords}
        groupId={groupId}
      />
      {replyTo !== null && (
        <div className="mx-3 -mb-1 flex items-center justify-between gap-2 rounded-t-lg border border-b-0 border-rail/60 bg-sidebar px-3 py-1.5 text-xs">
          <span className="min-w-0 truncate text-muted">
            {interpolate(t.dm.replyingTo, { name: nameOf(replyTo.author) })}
          </span>
          <button
            type="button"
            aria-label={t.dm.cancelReply}
            title={t.dm.cancelReply}
            onClick={() => setReplyTo(null)}
            className="flex shrink-0 items-center justify-center rounded-full p-0.5 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar active:scale-90"
          >
            <CloseIcon size={12} />
          </button>
        </div>
      )}
      <MessageInput
        placeholder={interpolate(t.threads.inputPlaceholder, { name: thread.name })}
        groupId={groupId}
        typingTarget={{ kind: 'group', groupId, channelId: threadId }}
        focusKey={replyTo?.msg_id ?? null}
        automodWords={automodWords}
        onSend={async (text, attachments) => {
          await send(groupId, threadId, text, replyTo?.msg_id, attachments);
          setReplyTo(null);
        }}
      />
      <TypingIndicator typingKey={groupTypingKey(groupId, threadId)} nameOf={nameOf} />
    </aside>
  );
}
