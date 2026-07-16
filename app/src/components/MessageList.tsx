import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { DeliveryState, GroupThread } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
import { formatDay, formatTimestamp, formatTimestampCompact } from '../lib/format';
import { extractInviteLink } from '../lib/invite';
import { isEditableTarget, useContextMenu } from '../stores/contextMenu';
import { useDms } from '../stores/dms';
import {
  useFriends,
  avatarDecorationOf,
  avatarOf,
  displayNameOf,
} from '../stores/friends';
import {
  hasPerm,
  PERMISSIONS,
  pollOf,
  serverAvatarOf,
  useGroups,
} from '../stores/groups';
import { useMessageEdit } from '../stores/messageEdit';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT, type AncrePopover } from '../stores/ui';
import { AttachmentRow } from './Attachments';
import { Avatar } from './Avatar';
import { BodyText } from './BodyText';
import { ForwardPicker } from './ForwardPicker';
import { InviteEmbed } from './InviteEmbed';
import { MessageActions } from './MessageActions';
import { MessageEditor } from './MessageEditor';
import { buildMessageItems, buildUserItems, type MessageMenuDeps } from './messageMenus';
import {
  displayText,
  firstUnreadIndex,
  messageLink,
  type DisplayMessage,
  type MessageListActions,
} from './messageModel';
import { MessageQuote } from './MessageQuote';
import { PollCard } from './PollCard';
import { ReactionRow } from './Reactions';
import { messageOf } from './server/controls';
import { StickerImage } from './StickerImage';

// Ré-exports de l'API historique du module : les consommateurs (vues, tests)
// importent modèle et lien depuis `MessageList` — voir `messageModel`.
export { messageLink };
export type { DisplayMessage, MessageListActions };

/** Fenêtre de regroupement de messages consécutifs du même auteur. */
const GROUP_WINDOW_MS = 5 * 60 * 1000;

/** Distance au haut du fil (px) sous laquelle on charge la page précédente. */
const LOAD_OLDER_THRESHOLD_PX = 80;

/** Distance au bas (px) sous laquelle les nouveaux messages restent suivis. */
const FOLLOW_BOTTOM_THRESHOLD_PX = 80;

/** Durée de la surbrillance d'un message atteint par un saut (ms). */
const HIGHLIGHT_MS = 1600;

function messageScrollBehavior(): ScrollBehavior {
  const preference = useUi.getState().reducedMotion;
  if (preference === 'on') return 'auto';
  return window.matchMedia?.('(prefers-reduced-motion: reduce)')?.matches === true
    ? 'auto'
    : 'smooth';
}

/** État de livraison effectif : le champ explicite prime, l'ack le complète. */
function deliveryOf(message: DisplayMessage): DeliveryState {
  if (message.delivery !== undefined) return message.delivery;
  return message.acked === false ? 'pending' : 'sent';
}

function sameDay(a: number, b: number): boolean {
  const da = new Date(a);
  const db = new Date(b);
  return (
    da.getFullYear() === db.getFullYear() &&
    da.getMonth() === db.getMonth() &&
    da.getDate() === db.getDate()
  );
}

export interface MessageListProps {
  messages: DisplayMessage[];
  hasMore?: boolean;
  onLoadOlder?: () => void;
  actions?: MessageListActions;
  pinnedIds?: ReadonlySet<string>;
  colorOf?: (author: string) => string | null;
  emojiMap?: ReadonlyMap<string, string> | undefined;
  knownMentions?: ReadonlySet<string> | undefined;
  automodWords?: readonly string[] | undefined;
  groupId?: string | null | undefined;
  scrollTarget?: { msgId: string; nonce: number } | null;
  threads?: readonly GroupThread[] | undefined;
  onOpenThread?: ((message: DisplayMessage) => void) | undefined;
  /**
   * Position lue capturée à l'ouverture (lamport) : le séparateur « nouveaux
   * messages » s'insère avant le premier message d'autrui au-delà. `null`/`0`
   * (ou aucun message au-delà) ⇒ pas de séparateur. Figé côté vue (one-shot).
   */
  dividerLamport?: number | null;
  /**
   * Mode sélection de messages (suppression groupée, salons) : quand `active`,
   * chaque message porte une case à cocher et les actions de survol sont
   * masquées. `null`/absent = mode inactif.
   */
  selection?:
    | {
        active: boolean;
        selected: ReadonlySet<string>;
        onToggle: (msgId: string) => void;
      }
    | undefined;
  /**
   * Entrée en mode sélection depuis le menu contextuel d'un message (câblée par
   * la vue groupe pour un porteur de `MANAGE_MESSAGES`). Absent = pas d'entrée.
   */
  onStartSelection?: ((message: DisplayMessage) => void) | undefined;
}

export function MessageList({
  messages,
  hasMore,
  onLoadOlder,
  actions,
  pinnedIds,
  colorOf,
  emojiMap,
  knownMentions,
  automodWords,
  groupId = null,
  scrollTarget = null,
  threads,
  onOpenThread,
  dividerLamport = null,
  selection,
  onStartSelection,
}: MessageListProps) {
  const lang = useUi((s) => s.lang);
  const timeFormat = useUi((s) => s.timeFormat);
  const openProfile = useUi((s) => s.openProfile);
  const requestJump = useUi((s) => s.requestJump);
  const requestMentionInsert = useUi((s) => s.requestMentionInsert);
  const toast = useUi((s) => s.toast);
  const view = useUi((s) => s.view);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const t = useT();
  // Rôles du groupe (nom minuscule → couleur) pour les pastilles `@rôle`.
  const groupRoles = useGroups((s) =>
    groupId !== null ? s.states[groupId]?.roles : undefined,
  );
  const roleColors = useMemo(() => {
    const map = new Map<string, number>();
    for (const role of groupRoles ?? []) map.set(role.name.toLowerCase(), role.color);
    return map;
  }, [groupRoles]);
  // Pseudos de serveur (pubkey → pseudo) : priment sur le pseudo global dans
  // les en-têtes et citations. Vide hors contexte de groupe (MP).
  const groupMembers = useGroups((s) =>
    groupId !== null ? s.states[groupId]?.members : undefined,
  );
  const nicknames = useMemo(() => {
    const map = new Map<string, string>();
    for (const member of groupMembers ?? []) {
      const nick = member.nickname;
      if (nick != null && nick.trim() !== '') map.set(member.pubkey, nick);
    }
    return map;
  }, [groupMembers]);
  // État complet du groupe (sondages, permissions) : nécessaire à la carte de
  // sondage — la question/les options viennent du corps du message, le
  // dépouillement en direct de `groups.state.polls`.
  const groupState = useGroups((s) => (groupId !== null ? s.states[groupId] : undefined));
  const votePoll = useGroups((s) => s.votePoll);
  const closePoll = useGroups((s) => s.closePoll);
  const [forwarding, setForwarding] = useState<DisplayMessage | null>(null);
  const editRequest = useMessageEdit((s) => s.request);
  const clearEditRequest = useMessageEdit((s) => s.clearEditRequest);

  /** Copie une valeur puis confirme (`successText`) ou signale l'échec. */
  const copyWithToast = (value: string, successText: string): void => {
    copyToClipboard(
      value,
      () => toast('info', successText),
      () => toast('error', t.errors.actionFailed),
    );
  };

  /** Copie le lien `accord:` du message et confirme par un toast. */
  const copyLink = (message: DisplayMessage): void => {
    const link = messageLink(view, message.msg_id);
    if (link === null) return;
    copyWithToast(link, t.dm.linkCopied);
  };
  // Accusés de lecture : uniquement en conversation directe (jamais en salon).
  const dmPeer = groupId === null && view.kind === 'dm' ? view.peer : null;
  const peerReadLamport = useDms((s) =>
    dmPeer === null ? undefined : s.peerRead[dmPeer],
  );
  const dmMessages = useDms((s) =>
    dmPeer === null ? undefined : s.conversations[dmPeer],
  );
  const containerRef = useRef<HTMLDivElement>(null);
  const firstIdRef = useRef<string | null>(null);
  const lastIdRef = useRef<string | null>(null);
  const scrollConversationRef = useRef<string | null>(null);
  const followsBottomRef = useRef(true);
  const anchorRef = useRef<{ height: number; top: number } | null>(null);
  /** Message en cours d'édition en place (null : aucun). */
  const [editingId, setEditingId] = useState<string | null>(null);
  /** Rangées rendues par `msg_id`, pour cibler le défilement d'un saut. */
  const rowRefs = useRef(new Map<string, HTMLDivElement>());
  /** Message en surbrillance après un saut (null : aucun). */
  const [highlightId, setHighlightId] = useState<string | null>(null);
  /** Dernier message dont l'apparition doit être animée (voir `.msg-append`). */
  const [appendedId, setAppendedId] = useState<string | null>(null);
  const prevConversationRef = useRef<string | null>(null);
  const prevAppendLastIdRef = useRef<string | null>(null);

  // Saut vers un message : défilement centré puis surbrillance brève. Le
  // `nonce` rejoue l'animation même pour une cible identique.
  useEffect(() => {
    if (scrollTarget === null) return;
    const el = rowRefs.current.get(scrollTarget.msgId);
    if (el === undefined) return;
    try {
      el.scrollIntoView({ block: 'center', behavior: messageScrollBehavior() });
    } catch {
      // Environnement de test sans mise en page (jsdom) : défilement ignoré.
    }
    setHighlightId(scrollTarget.msgId);
    const timer = window.setTimeout(() => setHighlightId(null), HIGHLIGHT_MS);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollTarget?.nonce]);

  // Requête externe d'édition en place (composeur vide + flèche Haut, voir
  // `MessageInput`) : ouvre le même éditeur que le menu contextuel, sans
  // dupliquer sa logique. Ignorée si le message ciblé n'est pas dans ce fil
  // (mauvaise conversation) ; toujours consommée pour ne jamais reboucler.
  useEffect(() => {
    if (editRequest === null) return;
    if (messages.some((m) => m.msg_id === editRequest.msgId)) {
      setEditingId(editRequest.msgId);
    }
    clearEditRequest();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [editRequest]);

  // Les messages de pur contenu (texte / sticker / supprimés) sont affichés ;
  // éditions, réactions et méta sont des mutations déjà appliquées.
  const visible = messages.filter(
    (m) =>
      m.deleted ||
      m.body.type === 'text' ||
      m.body.type === 'sticker' ||
      m.body.type === 'poll' ||
      m.body.type === 'unknown',
  );

  /** Index msg_id → message, pour retrouver les messages cités. */
  const byId = new Map(messages.map((m) => [m.msg_id, m]));

  const firstId = visible[0]?.msg_id ?? null;
  const lastId = visible[visible.length - 1]?.msg_id ?? null;

  // Identité de la conversation affichée (MP ou salon), pour distinguer un
  // véritable message entrant en fin de fil (à animer, voir `.msg-append`
  // dans global.css) d'un changement de salon/MP ou d'un chargement
  // d'historique — jamais animés.
  const conversationKey =
    view.kind === 'dm'
      ? `dm:${view.peer}`
      : view.kind === 'group'
        ? `group:${view.groupId}:${view.channelId ?? ''}`
        : 'autre';

  // « Vu » : dernier de ses propres messages couvert par l'accusé de lecture
  // du pair (lamport lu depuis le store des MP, l'enveloppe affichée n'en a pas).
  let seenMsgId: string | null = null;
  if (
    dmPeer !== null &&
    peerReadLamport !== undefined &&
    dmMessages !== undefined &&
    self !== null
  ) {
    const lamportById = new Map(dmMessages.map((m) => [m.msg_id, m.lamport]));
    for (const m of visible) {
      if (m.deleted || m.author !== self.pubkey) continue;
      const lamport = lamportById.get(m.msg_id);
      if (lamport !== undefined && lamport <= peerReadLamport) seenMsgId = m.msg_id;
    }
  }

  useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const anchor = anchorRef.current;
    const conversationChanged = scrollConversationRef.current !== conversationKey;
    const firstIdChanged = firstIdRef.current !== null && firstId !== firstIdRef.current;
    if (conversationChanged) {
      anchorRef.current = null;
      el.scrollTop = el.scrollHeight;
      followsBottomRef.current = true;
    } else if (anchor !== null && firstIdChanged) {
      el.scrollTop = anchor.top + (el.scrollHeight - anchor.height);
      anchorRef.current = null;
    } else if (lastId !== lastIdRef.current) {
      // Premier rendu, ou arrivée pendant que la lecture suivait déjà le bas :
      // on colle en bas. Une personne remontée dans l'historique garde sa
      // position, même lorsqu'un nouveau message est reçu.
      if (lastIdRef.current === null || followsBottomRef.current) {
        el.scrollTop = el.scrollHeight;
        followsBottomRef.current = true;
        anchorRef.current = null;
      } else if (anchor !== null) {
        anchorRef.current = { height: el.scrollHeight, top: el.scrollTop };
      }
    }
    scrollConversationRef.current = conversationKey;
    firstIdRef.current = firstId;
    lastIdRef.current = lastId;
  }, [conversationKey, firstId, lastId]);

  // Message dont l'arrivée doit jouer l'entrée `.msg-append` : uniquement un
  // nouveau dernier message dans la même conversation (jamais le premier
  // rendu, un changement de salon/MP, ni une page d'historique chargée).
  useLayoutEffect(() => {
    const sameConversation = prevConversationRef.current === conversationKey;
    const genuineAppend =
      sameConversation &&
      prevAppendLastIdRef.current !== null &&
      lastId !== null &&
      lastId !== prevAppendLastIdRef.current;
    if (genuineAppend) setAppendedId(lastId);
    prevConversationRef.current = conversationKey;
    prevAppendLastIdRef.current = lastId;
  }, [conversationKey, lastId]);

  const handleScroll = (): void => {
    const el = containerRef.current;
    if (!el) return;
    followsBottomRef.current =
      el.scrollHeight - el.scrollTop - el.clientHeight <= FOLLOW_BOTTOM_THRESHOLD_PX;
    if (hasMore !== true || onLoadOlder === undefined) return;
    if (el.scrollTop > LOAD_OLDER_THRESHOLD_PX) return;
    anchorRef.current = { height: el.scrollHeight, top: el.scrollTop };
    onLoadOlder();
  };

  const nameOf = (author: string): string => {
    const nick = nicknames.get(author);
    if (self && author === self.pubkey) {
      return `${nick ?? selfDisplayName(self)}`;
    }
    return nick ?? displayNameOf(contacts, author);
  };

  /**
   * Hash d'avatar d'un auteur : avatar de serveur self-service s'il est
   * défini pour ce groupe (`serverAvatarOf`), sinon l'avatar global (soi-même,
   * ou le contact ami connu).
   */
  const avatarHashOf = (author: string): string | null => {
    const globalAvatar =
      self && author === self.pubkey ? self.avatar : avatarOf(contacts, author);
    if (groupMembers === undefined) return globalAvatar;
    return serverAvatarOf({ members: groupMembers }, contacts, author) ?? globalAvatar;
  };

  /** Décoration globale de profil (indépendante d'un éventuel avatar serveur). */
  const decorationOf = (author: string): string | null =>
    self && author === self.pubkey
      ? self.avatar_decoration
      : avatarDecorationOf(contacts, author);

  /** Ouvre la carte de profil, ancrée sur l'élément cliqué. */
  const ouvrirProfil = (author: string, target: HTMLElement): void => {
    const r = target.getBoundingClientRect();
    const ancre: AncrePopover = {
      top: r.top,
      left: r.left,
      bottom: r.bottom,
      right: r.right,
    };
    openProfile(author, ancre, groupId);
  };

  /** Contexte partagé des menus contextuels du fil (voir `messageMenus`). */
  const menuDeps: MessageMenuDeps = {
    t,
    selfPubkey: self?.pubkey ?? null,
    contacts,
    actions,
    nameOf,
    copyWithToast,
    requestMentionInsert,
    openProfile: ouvrirProfil,
    onForward: setForwarding,
    onEditInPlace: setEditingId,
    threads,
    onOpenThread,
    onStartSelection,
  };

  const selectionActive = selection?.active === true;
  // Séparateur « nouveaux messages » : index (dans `visible`) du premier
  // message d'autrui au-delà de la position lue capturée à l'ouverture.
  const dividerIndex = firstUnreadIndex(visible, dividerLamport, self?.pubkey ?? null);

  /** Défile jusqu'au séparateur « nouveaux messages » (bouton de saut). */
  const jumpToUnread = (): void => {
    if (dividerIndex < 0) return;
    const target = visible[dividerIndex];
    if (target === undefined) return;
    const el = rowRefs.current.get(target.msg_id);
    try {
      el?.scrollIntoView({ block: 'center', behavior: messageScrollBehavior() });
    } catch {
      // jsdom sans mise en page : défilement ignoré.
    }
  };

  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <div
        ref={containerRef}
        onScroll={handleScroll}
        className="view-enter min-h-0 flex-1 overflow-y-auto pb-4"
        role="log"
        aria-live="polite"
      >
        {visible.length === 0 && (
          <p className="py-16 text-center text-sm text-muted">{t.dm.empty}</p>
        )}
        {visible.map((m, i) => {
          const prev = visible[i - 1];
          const newDay = !prev || !sameDay(prev.sent_ms, m.sent_ms);
          const isReply = m.body.type === 'text' && m.body.reply_to !== null;
          // Une réponse ré-affiche l'en-tête pour accueillir la citation.
          const grouped =
            !newDay &&
            !isReply &&
            prev !== undefined &&
            prev.author === m.author &&
            m.sent_ms - prev.sent_ms < GROUP_WINDOW_MS;
          const name = nameOf(m.author);
          const nameColor = colorOf?.(m.author) ?? null;
          const isEditing = editingId === m.msg_id;
          const actionable =
            actions !== undefined &&
            !isEditing &&
            !selectionActive &&
            !m.deleted &&
            (m.body.type === 'text' ||
              m.body.type === 'sticker' ||
              m.body.type === 'poll');
          const isSelected =
            selectionActive && (selection?.selected.has(m.msg_id) ?? false);
          // Message sans texte (pièces jointes seules) : pas de corps vide.
          const hasAttachments = !m.deleted && (m.attachments?.length ?? 0) > 0;
          const corpsVide = !m.deleted && hasAttachments && displayText(m) === '';
          const pollBody = !m.deleted && m.body.type === 'poll' ? m.body : null;
          const inviteLink =
            !m.deleted && m.body.type === 'text'
              ? extractInviteLink(m.edited ?? m.body.text)
              : null;
          const isOwn = self !== null && m.author === self.pubkey;
          const pinned = pinnedIds?.has(m.msg_id) ?? false;
          const delivery = deliveryOf(m);
          // Fil ancré sur ce message : pastille cliquable sous le corps.
          const rootThread =
            onOpenThread === undefined
              ? undefined
              : threads?.find((th) => th.root_msg === m.msg_id);

          return (
            <div key={m.msg_id}>
              {i === dividerIndex && (
                <div
                  className="mx-4 mb-1 mt-4 flex items-center gap-3"
                  role="separator"
                  aria-label={t.unread.newMessages}
                >
                  <div className="h-px flex-1 bg-red/50" />
                  <span className="shrink-0 text-[11px] font-semibold uppercase tracking-wide text-red">
                    {t.unread.newMessages}
                  </span>
                  <div className="h-px flex-1 bg-red/50" />
                </div>
              )}
              {newDay && (
                <div className="mx-4 mb-1 mt-4 flex items-center gap-3" role="separator">
                  <div className="h-px flex-1 bg-input" />
                  <span className="rounded-full bg-chat-hover px-2.5 py-1 text-[11px] font-medium text-faint">
                    {formatDay(m.sent_ms, lang)}
                  </span>
                  <div className="h-px flex-1 bg-input" />
                </div>
              )}
              {/* Espacement piloté par la densité (variables CSS, global.css). */}
              <div
                ref={(el) => {
                  if (el === null) rowRefs.current.delete(m.msg_id);
                  else rowRefs.current.set(m.msg_id, el);
                }}
                data-msg-id={m.msg_id}
                className={`group relative mx-2 flex gap-4 rounded-md px-2 transition-colors duration-fast hover:bg-chat-hover ${
                  grouped
                    ? 'py-[var(--message-pad-y-grouped)]'
                    : 'mt-[var(--message-gap)] py-[var(--message-pad-y)]'
                } ${highlightId === m.msg_id ? 'msg-flash' : ''} ${
                  appendedId === m.msg_id ? 'msg-append' : ''
                } ${m.mentions_me ? 'msg-mention' : ''} ${
                  isSelected ? 'bg-blurple/10' : ''
                }`}
                onContextMenu={(e) => {
                  // Édition en place (textarea) : laisse le clic droit natif
                  // (copier/coller) plutôt que d'ouvrir le menu du message.
                  if (isEditableTarget(e.target)) return;
                  e.preventDefault();
                  useContextMenu
                    .getState()
                    .openMenu(
                      e.clientX,
                      e.clientY,
                      buildMessageItems(menuDeps, m, isOwn, pinned),
                    );
                }}
              >
                {selectionActive && (
                  <input
                    type="checkbox"
                    aria-label={t.purge.selectMessage}
                    checked={isSelected}
                    onChange={() => selection?.onToggle(m.msg_id)}
                    className="mt-1 h-4 w-4 shrink-0 cursor-pointer self-start accent-blurple"
                  />
                )}
                {actionable && (
                  <MessageActions
                    canEdit={isOwn && m.body.type === 'text'}
                    canDelete={isOwn || actions.canModerate === true}
                    onReact={(emoji) => actions.onReact(m, emoji)}
                    onReply={
                      actions.onReply === undefined
                        ? undefined
                        : () => actions.onReply?.(m)
                    }
                    onEdit={() => setEditingId(m.msg_id)}
                    onDelete={() => actions.onDelete(m)}
                    onForward={() => setForwarding(m)}
                    onCopyLink={() => copyLink(m)}
                    onTogglePin={
                      actions.onTogglePin === undefined
                        ? undefined
                        : () => actions.onTogglePin?.(m, pinned)
                    }
                    pinned={pinned}
                    groupId={groupId}
                  />
                )}
                {grouped ? (
                  <div className="w-10 shrink-0 overflow-hidden whitespace-nowrap pt-1 text-right text-[10px] leading-5 tracking-tight text-faint opacity-0 group-hover:opacity-100">
                    {formatTimestampCompact(m.sent_ms, lang, undefined, timeFormat)}
                  </div>
                ) : (
                  <button
                    type="button"
                    aria-label={interpolate(t.profil.openProfile, { name })}
                    onClick={(e) => ouvrirProfil(m.author, e.currentTarget)}
                    onContextMenu={(e) => {
                      e.preventDefault();
                      e.stopPropagation();
                      useContextMenu
                        .getState()
                        .openMenu(
                          e.clientX,
                          e.clientY,
                          buildUserItems(menuDeps, m.author, e.currentTarget),
                        );
                    }}
                    className="shrink-0 self-start rounded-full focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
                  >
                    <Avatar
                      id={m.author}
                      name={name}
                      size={40}
                      avatarHash={avatarHashOf(m.author)}
                      hint={m.author}
                      decoration={decorationOf(m.author)}
                    />
                  </button>
                )}
                <div className="min-w-0 flex-1">
                  {!grouped && (
                    <>
                      {isReply && m.body.type === 'text' && m.body.reply_to !== null && (
                        <MessageQuote
                          quoted={byId.get(m.body.reply_to)}
                          nameOf={nameOf}
                          onJump={() => {
                            if (m.body.type === 'text' && m.body.reply_to !== null) {
                              requestJump(view, m.body.reply_to);
                            }
                          }}
                        />
                      )}
                      <div className="flex min-w-0 items-baseline gap-2">
                        <button
                          type="button"
                          onClick={(e) => ouvrirProfil(m.author, e.currentTarget)}
                          onContextMenu={(e) => {
                            e.preventDefault();
                            e.stopPropagation();
                            useContextMenu
                              .getState()
                              .openMenu(
                                e.clientX,
                                e.clientY,
                                buildUserItems(menuDeps, m.author, e.currentTarget),
                              );
                          }}
                          className="min-w-0 truncate font-semibold text-header hover:underline focus-visible:underline focus-visible:outline-none"
                          style={nameColor !== null ? { color: nameColor } : undefined}
                        >
                          {name}
                        </button>
                        <span className="shrink-0 whitespace-nowrap text-xs text-faint">
                          {formatTimestamp(m.sent_ms, lang, undefined, timeFormat)}
                        </span>
                        {pinned && (
                          <svg
                            aria-hidden
                            width="12"
                            height="12"
                            viewBox="0 0 24 24"
                            fill="none"
                            stroke="currentColor"
                            strokeWidth={2}
                            strokeLinecap="round"
                            strokeLinejoin="round"
                            className="shrink-0 text-yellow/80"
                          >
                            <line x1="12" x2="12" y1="17" y2="22" />
                            <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z" />
                          </svg>
                        )}
                        {isOwn && delivery === 'pending' && (
                          <span className="text-xs italic text-faint">
                            {t.dm.pending}
                          </span>
                        )}
                      </div>
                    </>
                  )}
                  {isEditing ? (
                    <MessageEditor
                      initial={displayText(m) ?? ''}
                      onSave={(text) => {
                        setEditingId(null);
                        if (text !== displayText(m)) actions?.onEdit(m, text);
                      }}
                      onCancel={() => setEditingId(null)}
                    />
                  ) : !m.deleted && m.body.type === 'sticker' ? (
                    <div className="leading-6">
                      <StickerImage
                        name={m.body.name}
                        merkleRoot={m.body.merkle_root}
                        hint={m.author}
                      />
                    </div>
                  ) : pollBody !== null ? (
                    <PollCard
                      question={pollBody.question}
                      options={pollBody.options}
                      poll={pollOf(groupState, pollBody.poll_id)}
                      resultsAvailable={groupState?.polls !== undefined}
                      canClose={
                        groupState !== undefined &&
                        (isOwn ||
                          hasPerm(groupState.my_permissions, PERMISSIONS.MANAGE_CHANNELS))
                      }
                      onVote={(optionIndex) => {
                        if (groupId === null) return;
                        votePoll(groupId, pollBody.poll_id, optionIndex).catch(() =>
                          toast('error', t.errors.actionFailed),
                        );
                      }}
                      onClose={() => {
                        if (groupId === null) return;
                        closePoll(groupId, pollBody.poll_id).catch((e: unknown) =>
                          toast('error', messageOf(e, t.errors.actionFailed)),
                        );
                      }}
                    />
                  ) : (
                    !corpsVide && (
                      <div className="leading-6 text-norm">
                        <BodyText
                          message={m}
                          emojiMap={emojiMap}
                          knownMentions={knownMentions}
                          roleColors={roleColors}
                          automodWords={automodWords}
                        />
                      </div>
                    )
                  )}
                  {hasAttachments && (
                    <AttachmentRow pieces={m.attachments ?? []} hint={m.author} />
                  )}
                  {inviteLink !== null && <InviteEmbed link={inviteLink} />}
                  {rootThread !== undefined && (
                    <button
                      type="button"
                      onClick={() => onOpenThread?.(m)}
                      className="mt-1 inline-flex max-w-full items-center gap-1.5 rounded-full bg-chat-hover px-2.5 py-1 text-xs font-medium text-muted transition-colors duration-fast hover:bg-input hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat active:scale-95"
                    >
                      <span aria-hidden>💬</span>
                      <span className="truncate">{rootThread.name}</span>
                      {rootThread.archived && (
                        <span className="shrink-0 text-faint">{t.threads.archived}</span>
                      )}
                    </button>
                  )}
                  {!m.deleted && (
                    <ReactionRow
                      reactions={m.reactions ?? []}
                      selfPubkey={self?.pubkey ?? null}
                      onToggle={
                        actions === undefined
                          ? undefined
                          : (emoji) => actions.onReact(m, emoji)
                      }
                      emojis={emojiMap}
                      hint={m.author}
                      nameOf={nameOf}
                      avatarHashOf={avatarHashOf}
                      avatarDecorationOf={decorationOf}
                      onOpenAuthor={ouvrirProfil}
                    />
                  )}
                  {m.msg_id === seenMsgId && (
                    <div className="mt-0.5 text-[10px] font-medium uppercase tracking-wide text-faint">
                      {t.dm.seen}
                    </div>
                  )}
                  {isOwn && delivery === 'failed' && actions?.onRetry !== undefined && (
                    <div className="mt-0.5 flex items-center gap-1.5 text-xs text-red">
                      <svg
                        width="14"
                        height="14"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth={2}
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        aria-hidden
                      >
                        <circle cx="12" cy="12" r="10" />
                        <line x1="12" x2="12" y1="8" y2="12" />
                        <line x1="12" x2="12.01" y1="16" y2="16" />
                      </svg>
                      <span>{t.dm.sendFailed}</span>
                      <button
                        type="button"
                        onClick={() => actions.onRetry?.(m)}
                        className="font-medium underline-offset-2 hover:underline"
                      >
                        {t.dm.retry}
                      </button>
                    </div>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
      {dividerIndex >= 0 && (
        <button
          type="button"
          onClick={jumpToUnread}
          className="glass-strong popover-enter absolute left-1/2 top-2 z-10 -translate-x-1/2 rounded-full px-3 py-1 text-xs font-medium text-red shadow-1 transition-transform duration-fast hover:scale-105 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-chat active:scale-95"
        >
          {t.unread.jumpToUnread} ↓
        </button>
      )}
      {forwarding !== null && (
        <ForwardPicker
          text={displayText(forwarding) ?? ''}
          attachments={forwarding.attachments}
          onClose={() => setForwarding(null)}
        />
      )}
    </div>
  );
}
