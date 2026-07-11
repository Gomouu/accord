/**
 * Fil de messages : séparateurs de jour, regroupement par auteur (fenêtre de
 * 5 minutes), horodatages, mentions d'édition/suppression — disposition
 * calquée sur Discord. Le défilement vers le haut charge l'historique plus
 * ancien (pagination `before_lamport`) en préservant la position de lecture.
 *
 * Quand `actions` est fourni (MP et salons), chaque message expose au survol
 * une barre d'actions : réaction, réponse (MP uniquement), épinglage (salons
 * avec MANAGE_MESSAGES), édition (auteur seul) et suppression (auteur, ou
 * modération via `canModerate`). L'édition se fait en place et les réactions
 * s'affichent en pastilles sous le corps. `colorOf` colore le nom des
 * auteurs (couleur du rôle le plus haut en salon).
 */

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { DeliveryState, FileAttachment, MsgBody, Reaction } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
import { formatDay, formatTimestamp, formatTimestampCompact } from '../lib/format';
import { isEditableTarget, useContextMenu, type ContextMenuItem } from '../stores/contextMenu';
import { useDms } from '../stores/dms';
import { useFriends, avatarOf, displayNameOf } from '../stores/friends';
import { useGroups } from '../stores/groups';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT, type AncrePopover, type View } from '../stores/ui';
import { AttachmentRow } from './Attachments';
import { Avatar } from './Avatar';
import {
  CopyMenuIcon,
  DeleteMenuIcon,
  EditMenuIcon,
  EnvelopeMenuIcon,
  ForwardMenuIcon,
  MentionMenuIcon,
  PinMenuIcon,
  ProfileMenuIcon,
  ReplyMenuIcon,
} from './ContextMenu';
import { ForwardPicker } from './ForwardPicker';
import { MarkdownText } from './MarkdownText';
import { MessageActions } from './MessageActions';
import { ReactionRow } from './Reactions';

export interface DisplayMessage {
  msg_id: string;
  author: string;
  sent_ms: number;
  deleted: boolean;
  body: MsgBody;
  edited: string | null;
  acked?: boolean;
  /** État de livraison sortante (MP uniquement) ; absent = considéré envoyé. */
  delivery?: DeliveryState;
  reactions?: Reaction[];
  /** Pièces jointes de l'enveloppe (`[]` ou absent si aucune). */
  attachments?: FileAttachment[];
}

/** Actions de message ; leur absence masque toute la barre. */
export interface MessageListActions {
  onReact: (message: DisplayMessage, emoji: string) => void;
  /** Réponse citée — absente dans les salons (non prévue par l'API). */
  onReply?: (message: DisplayMessage) => void;
  onEdit: (message: DisplayMessage, text: string) => void;
  onDelete: (message: DisplayMessage) => void;
  /** Modération : autorise la suppression des messages d'autrui. */
  canModerate?: boolean;
  /** Épinglage — `pinned` reflète l'état courant du message. */
  onTogglePin?: (message: DisplayMessage, pinned: boolean) => void;
  /** Relance d'un envoi échoué (MP uniquement) ; absente = pas d'affordance. */
  onRetry?: (message: DisplayMessage) => void;
}

/** Fenêtre de regroupement de messages consécutifs du même auteur. */
const GROUP_WINDOW_MS = 5 * 60 * 1000;

/** Distance au haut du fil (px) sous laquelle on charge la page précédente. */
const LOAD_OLDER_THRESHOLD_PX = 80;

/** Durée de la surbrillance d'un message atteint par un saut (ms). */
const HIGHLIGHT_MS = 1600;

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

/** Texte affichable d'un message (dernière édition, sinon corps d'origine). */
function displayText(message: DisplayMessage): string | null {
  return message.edited ?? (message.body.type === 'text' ? message.body.text : null);
}

/**
 * Lien `accord:` copiable vers un message : `accord:msg/<conversation>/<id>`
 * où la conversation est `dm:<pair>` ou `group:<groupe>:<salon>`. Aucun
 * gestionnaire d'ouverture n'existe encore (copier suffit — voir le suivi).
 */
export function messageLink(view: View, msgId: string): string | null {
  if (view.kind === 'dm') return `accord:msg/dm:${view.peer}/${msgId}`;
  if (view.kind === 'group') {
    return `accord:msg/group:${view.groupId}:${view.channelId ?? ''}/${msgId}`;
  }
  return null;
}

function BodyText({
  message,
  emojiMap,
  knownMentions,
  roleColors,
}: {
  message: DisplayMessage;
  emojiMap?: ReadonlyMap<string, string> | undefined;
  knownMentions?: ReadonlySet<string> | undefined;
  roleColors?: ReadonlyMap<string, number> | undefined;
}) {
  const t = useT();
  if (message.deleted) {
    return <em className="text-faint">{t.dm.deletedMessage}</em>;
  }
  const text = displayText(message);
  if (text === null) {
    return <em className="text-faint">{t.dm.unsupported}</em>;
  }
  return (
    <span className="selectable whitespace-pre-wrap break-words">
      <MarkdownText
        text={text}
        emojis={emojiMap}
        knownMentions={knownMentions}
        roleColors={roleColors}
        hint={message.author}
      />
      {message.edited !== null && (
        <span className="ml-1 text-[10px] text-faint">{t.dm.edited}</span>
      )}
    </span>
  );
}

/** Aperçu du message cité, affiché au-dessus d'une réponse (clic = saut). */
function MessageQuote({
  quoted,
  nameOf,
  onJump,
}: {
  quoted: DisplayMessage | undefined;
  nameOf: (author: string) => string;
  onJump?: (() => void) | undefined;
}) {
  const t = useT();
  const snippet =
    quoted === undefined
      ? t.dm.quoteUnavailable
      : quoted.deleted
        ? t.dm.deletedMessage
        : (displayText(quoted) ?? t.dm.unsupported);

  const inner = (
    <>
      <span
        aria-hidden
        className="ml-1 h-2 w-6 shrink-0 rounded-tl-md border-l-2 border-t-2 border-input"
      />
      {quoted !== undefined && (
        <span className="shrink-0 font-medium text-header">{nameOf(quoted.author)}</span>
      )}
      <span className={`truncate ${quoted === undefined ? 'italic text-faint' : ''}`}>
        {snippet}
      </span>
    </>
  );

  const className = 'mb-0.5 flex items-center gap-1.5 text-xs text-muted';
  if (onJump === undefined) return <div className={className}>{inner}</div>;
  return (
    <button
      type="button"
      onClick={onJump}
      className={`${className} rounded-sm text-left hover:text-norm focus-visible:outline-none focus-visible:text-norm focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat`}
    >
      {inner}
    </button>
  );
}

/** Éditeur en place : Entrée enregistre, Échap annule. */
function MessageEditor({
  initial,
  onSave,
  onCancel,
}: {
  initial: string;
  onSave: (text: string) => void;
  onCancel: () => void;
}) {
  const t = useT();
  const [text, setText] = useState(initial);
  const ref = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
  }, []);

  const save = (): void => {
    const trimmed = text.trim();
    if (trimmed === '') return;
    onSave(trimmed);
  };

  return (
    <div className="py-1">
      <textarea
        ref={ref}
        aria-label={t.dm.edit}
        value={text}
        rows={1}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.shiftKey) {
            e.preventDefault();
            save();
          } else if (e.key === 'Escape') {
            onCancel();
          }
        }}
        className="max-h-48 min-h-[40px] w-full resize-none rounded-lg border border-rail/60 bg-input px-3 py-2 text-[15px] text-norm outline-none transition-colors duration-fast focus:border-blurple/50"
      />
      <div className="mt-0.5 text-[11px] text-faint">{t.dm.editHint}</div>
    </div>
  );
}

export interface MessageListProps {
  messages: DisplayMessage[];
  /** Vrai si un historique plus ancien peut encore être chargé. */
  hasMore?: boolean;
  /** Charge la page précédente (déclenché en approchant du haut du fil). */
  onLoadOlder?: () => void;
  /** Actions de message ; absentes = fil en lecture seule. */
  actions?: MessageListActions;
  /** Identifiants des messages épinglés du salon (état de l'épingle). */
  pinnedIds?: ReadonlySet<string>;
  /** Couleur du nom d'un auteur (rôle le plus haut) ; `null` = thème. */
  colorOf?: (author: string) => string | null;
  /** Émojis du serveur (nom → racine Merkle) pour corps et réactions. */
  emojiMap?: ReadonlyMap<string, string> | undefined;
  /** Noms connus (minuscules) mis en « pill » dans les mentions. */
  knownMentions?: ReadonlySet<string> | undefined;
  /** Contexte serveur (rôles au clic, émojis custom des réactions). */
  groupId?: string | null | undefined;
  /** Message à révéler (défilement + surbrillance) ; `nonce` rejoue le saut. */
  scrollTarget?: { msgId: string; nonce: number } | null;
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
  groupId = null,
  scrollTarget = null,
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
  const groupRoles = useGroups((s) => (groupId !== null ? s.states[groupId]?.roles : undefined));
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
  /** Message en cours de transfert (null : aucun). */
  const [forwarding, setForwarding] = useState<DisplayMessage | null>(null);

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
  const lastIdRef = useRef<string | null>(null);
  /** Position mémorisée avant l'insertion d'une page ancienne en tête. */
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
      el.scrollIntoView({ block: 'center', behavior: 'smooth' });
    } catch {
      // Environnement de test sans mise en page (jsdom) : défilement ignoré.
    }
    setHighlightId(scrollTarget.msgId);
    const timer = window.setTimeout(() => setHighlightId(null), HIGHLIGHT_MS);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scrollTarget?.nonce]);

  // Les messages de pur contenu (texte / supprimés) sont affichés ;
  // éditions, réactions et méta sont des mutations déjà appliquées.
  const visible = messages.filter(
    (m) => m.deleted || m.body.type === 'text' || m.body.type === 'unknown',
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
    anchorRef.current = null;
    if (lastId !== lastIdRef.current) {
      // Nouveau message en fin de fil (ou premier rendu) : on colle en bas.
      el.scrollTop = el.scrollHeight;
    } else if (anchor !== null) {
      // Page ancienne insérée en tête : le message lu reste sous les yeux.
      el.scrollTop = anchor.top + (el.scrollHeight - anchor.height);
    }
    lastIdRef.current = lastId;
  }, [firstId, lastId]);

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
    if (!el || hasMore !== true || onLoadOlder === undefined) return;
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

  /** Hash d'avatar d'un auteur : soi-même, sinon le contact ami connu. */
  const avatarHashOf = (author: string): string | null => {
    if (self && author === self.pubkey) return self.avatar;
    return avatarOf(contacts, author);
  };

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

  /**
   * Items du menu contextuel « utilisateur » (avatar, pseudo, entrée de la
   * liste des membres) : profil, mention, MP, copie d'identifiant — réutilise
   * `ouvrirProfil`, le pont de mention (`requestMentionInsert`) et l'action
   * d'ouverture de MP déjà existante (`ui.setView`).
   */
  const buildUserItems = (author: string, target: HTMLElement): ContextMenuItem[] => {
    const isSelfAuthor = self !== null && author === self.pubkey;
    const contact = contacts.find((c) => c.pubkey === author);
    const canMessage = !isSelfAuthor && contact?.state === 'friend';
    const items: ContextMenuItem[] = [
      {
        label: t.contextMenu.viewProfile,
        icon: <ProfileMenuIcon />,
        onClick: () => ouvrirProfil(author, target),
      },
      {
        label: interpolate(t.contextMenu.mention, { name: nameOf(author) }),
        icon: <MentionMenuIcon />,
        onClick: () => requestMentionInsert(nameOf(author)),
      },
    ];
    if (canMessage) {
      items.push({
        label: t.friends.sendDm,
        icon: <EnvelopeMenuIcon />,
        onClick: () => useUi.getState().setView({ kind: 'dm', peer: author }),
      });
    }
    items.push({
      label: t.contextMenu.copyUserId,
      icon: <CopyMenuIcon />,
      separatorBefore: true,
      onClick: () => copyWithToast(author, t.app.copied),
    });
    return items;
  };

  /**
   * Items du menu contextuel « message » : copie, mention de l'auteur, puis
   * réponse/transfert/épingle/édition/suppression — chacun réutilisant une
   * action déjà câblée par `actions` (ou omis si l'action n'existe pas ou
   * n'est pas permise), à l'image de la barre d'actions au survol.
   */
  const buildMessageItems = (message: DisplayMessage, isOwn: boolean, pinned: boolean): ContextMenuItem[] => {
    const text = !message.deleted ? displayText(message) : null;
    const canEdit = actions !== undefined && !message.deleted && isOwn;
    const canDelete =
      actions !== undefined &&
      !message.deleted &&
      (isOwn || actions.canModerate === true);
    const items: ContextMenuItem[] = [];
    if (text !== null && text !== '') {
      items.push({
        label: t.contextMenu.copyText,
        icon: <CopyMenuIcon />,
        onClick: () => copyWithToast(text, t.app.copied),
      });
    }
    items.push({
      label: t.contextMenu.copyMessageId,
      icon: <CopyMenuIcon />,
      onClick: () => copyWithToast(message.msg_id, t.app.copied),
    });
    if (actions !== undefined && !message.deleted) {
      items.push({
        label: interpolate(t.contextMenu.mention, { name: nameOf(message.author) }),
        icon: <MentionMenuIcon />,
        separatorBefore: true,
        onClick: () => requestMentionInsert(nameOf(message.author)),
      });
    }
    const flow: ContextMenuItem[] = [];
    if (!message.deleted && actions?.onReply !== undefined) {
      flow.push({
        label: t.dm.reply,
        icon: <ReplyMenuIcon />,
        onClick: () => actions.onReply?.(message),
      });
    }
    if (!message.deleted && actions !== undefined) {
      flow.push({
        label: t.dm.forward,
        icon: <ForwardMenuIcon />,
        onClick: () => setForwarding(message),
      });
    }
    if (!message.deleted && actions?.onTogglePin !== undefined) {
      flow.push({
        label: pinned ? t.serveur.unpin : t.serveur.pin,
        icon: <PinMenuIcon />,
        onClick: () => actions.onTogglePin?.(message, pinned),
      });
    }
    flow.forEach((item, i) => items.push(i === 0 ? { ...item, separatorBefore: true } : item));

    const management: ContextMenuItem[] = [];
    if (canEdit) {
      management.push({
        label: t.dm.edit,
        icon: <EditMenuIcon />,
        onClick: () => setEditingId(message.msg_id),
      });
    }
    if (canDelete) {
      management.push({
        label: t.dm.delete,
        icon: <DeleteMenuIcon />,
        danger: true,
        onClick: () => actions?.onDelete(message),
      });
    }
    management.forEach((item, i) =>
      items.push(i === 0 ? { ...item, separatorBefore: true } : item),
    );

    return items;
  };

  return (
    <>
    <div
      ref={containerRef}
      onScroll={handleScroll}
      className="view-enter flex-1 overflow-y-auto pb-4"
      role="log"
      aria-live="polite"
    >
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
          actions !== undefined && !isEditing && !m.deleted && m.body.type === 'text';
        // Message sans texte (pièces jointes seules) : pas de corps vide.
        const hasAttachments = !m.deleted && (m.attachments?.length ?? 0) > 0;
        const corpsVide = !m.deleted && hasAttachments && displayText(m) === '';
        const isOwn = self !== null && m.author === self.pubkey;
        const pinned = pinnedIds?.has(m.msg_id) ?? false;
        const delivery = deliveryOf(m);

        return (
          <div key={m.msg_id}>
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
              }`}
              onContextMenu={(e) => {
                // Édition en place (textarea) : laisse le clic droit natif
                // (copier/coller) plutôt que d'ouvrir le menu du message.
                if (isEditableTarget(e.target)) return;
                e.preventDefault();
                useContextMenu
                  .getState()
                  .openMenu(e.clientX, e.clientY, buildMessageItems(m, isOwn, pinned));
              }}
            >
              {actionable && (
                <MessageActions
                  canEdit={isOwn}
                  canDelete={isOwn || actions.canModerate === true}
                  onReact={(emoji) => actions.onReact(m, emoji)}
                  onReply={
                    actions.onReply === undefined ? undefined : () => actions.onReply?.(m)
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
                      .openMenu(e.clientX, e.clientY, buildUserItems(m.author, e.currentTarget));
                  }}
                  className="shrink-0 self-start rounded-full focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
                >
                  <Avatar
                    id={m.author}
                    name={name}
                    size={40}
                    avatarHash={avatarHashOf(m.author)}
                    hint={m.author}
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
                    <div className="flex items-baseline gap-2">
                      <button
                        type="button"
                        onClick={(e) => ouvrirProfil(m.author, e.currentTarget)}
                        onContextMenu={(e) => {
                          e.preventDefault();
                          e.stopPropagation();
                          useContextMenu
                            .getState()
                            .openMenu(e.clientX, e.clientY, buildUserItems(m.author, e.currentTarget));
                        }}
                        className="font-semibold text-header hover:underline focus-visible:underline focus-visible:outline-none"
                        style={nameColor !== null ? { color: nameColor } : undefined}
                      >
                        {name}
                      </button>
                      <span className="text-xs text-faint">
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
                        <span className="text-xs italic text-faint">{t.dm.pending}</span>
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
                ) : (
                  !corpsVide && (
                    <div className="leading-6 text-norm">
                      <BodyText
                        message={m}
                        emojiMap={emojiMap}
                        knownMentions={knownMentions}
                        roleColors={roleColors}
                      />
                    </div>
                  )
                )}
                {hasAttachments && (
                  <AttachmentRow pieces={m.attachments ?? []} hint={m.author} />
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
      {forwarding !== null && (
        <ForwardPicker
          text={displayText(forwarding) ?? ''}
          attachments={forwarding.attachments}
          onClose={() => setForwarding(null)}
        />
      )}
    </>
  );
}
