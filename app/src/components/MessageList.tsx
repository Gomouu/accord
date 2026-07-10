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

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { FileAttachment, MsgBody, Reaction } from '../lib/api';
import { formatDay, formatTimestamp } from '../lib/format';
import { useDms } from '../stores/dms';
import { useFriends, avatarOf, displayNameOf } from '../stores/friends';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT, type AncrePopover } from '../stores/ui';
import { AttachmentRow } from './Attachments';
import { Avatar } from './Avatar';
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
}

/** Fenêtre de regroupement de messages consécutifs du même auteur. */
const GROUP_WINDOW_MS = 5 * 60 * 1000;

/** Distance au haut du fil (px) sous laquelle on charge la page précédente. */
const LOAD_OLDER_THRESHOLD_PX = 80;

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

function BodyText({
  message,
  emojiMap,
  knownMentions,
}: {
  message: DisplayMessage;
  emojiMap?: ReadonlyMap<string, string> | undefined;
  knownMentions?: ReadonlySet<string> | undefined;
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
        hint={message.author}
      />
      {message.edited !== null && (
        <span className="ml-1 text-[10px] text-faint">{t.dm.edited}</span>
      )}
    </span>
  );
}

/** Aperçu du message cité, affiché au-dessus d'une réponse. */
function MessageQuote({
  quoted,
  nameOf,
}: {
  quoted: DisplayMessage | undefined;
  nameOf: (author: string) => string;
}) {
  const t = useT();
  const snippet =
    quoted === undefined
      ? t.dm.quoteUnavailable
      : quoted.deleted
        ? t.dm.deletedMessage
        : (displayText(quoted) ?? t.dm.unsupported);

  return (
    <div className="mb-0.5 flex items-center gap-1.5 text-xs text-muted">
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
    </div>
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
        className="max-h-48 min-h-[40px] w-full resize-none rounded-lg bg-input px-3 py-2 text-norm outline-none focus-visible:ring-2 focus-visible:ring-blurple"
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
}: MessageListProps) {
  const lang = useUi((s) => s.lang);
  const openProfile = useUi((s) => s.openProfile);
  const view = useUi((s) => s.view);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const t = useT();
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

  // Les messages de pur contenu (texte / supprimés) sont affichés ;
  // éditions, réactions et méta sont des mutations déjà appliquées.
  const visible = messages.filter(
    (m) => m.deleted || m.body.type === 'text' || m.body.type === 'unknown',
  );

  /** Index msg_id → message, pour retrouver les messages cités. */
  const byId = new Map(messages.map((m) => [m.msg_id, m]));

  const firstId = visible[0]?.msg_id ?? null;
  const lastId = visible[visible.length - 1]?.msg_id ?? null;

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

  const handleScroll = (): void => {
    const el = containerRef.current;
    if (!el || hasMore !== true || onLoadOlder === undefined) return;
    if (el.scrollTop > LOAD_OLDER_THRESHOLD_PX) return;
    anchorRef.current = { height: el.scrollHeight, top: el.scrollTop };
    onLoadOlder();
  };

  const nameOf = (author: string): string => {
    if (self && author === self.pubkey) {
      return `${selfDisplayName(self)} (${t.app.you})`;
    }
    return displayNameOf(contacts, author);
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

  return (
    <div
      ref={containerRef}
      onScroll={handleScroll}
      className="flex-1 overflow-y-auto pb-4"
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

        return (
          <div key={m.msg_id}>
            {newDay && (
              <div className="mx-4 mb-1 mt-4 flex items-center gap-3" role="separator">
                <div className="h-px flex-1 bg-input" />
                <span className="text-xs font-semibold text-faint">
                  {formatDay(m.sent_ms, lang)}
                </span>
                <div className="h-px flex-1 bg-input" />
              </div>
            )}
            {/* Espacement piloté par la densité (variables CSS, global.css). */}
            <div
              className={`group relative flex gap-4 px-4 hover:bg-chat-hover ${
                grouped
                  ? 'py-[var(--message-pad-y-grouped)]'
                  : 'mt-[var(--message-gap)] py-[var(--message-pad-y)]'
              }`}
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
                <div className="w-10 shrink-0 pt-1 text-right text-[10px] leading-5 text-faint opacity-0 group-hover:opacity-100">
                  {formatTimestamp(m.sent_ms, lang)}
                </div>
              ) : (
                <button
                  type="button"
                  aria-label={interpolate(t.profil.openProfile, { name })}
                  onClick={(e) => ouvrirProfil(m.author, e.currentTarget)}
                  className="shrink-0 self-start rounded-full focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
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
                      <MessageQuote quoted={byId.get(m.body.reply_to)} nameOf={nameOf} />
                    )}
                    <div className="flex items-baseline gap-2">
                      <button
                        type="button"
                        onClick={(e) => ouvrirProfil(m.author, e.currentTarget)}
                        className="font-medium text-header hover:underline focus-visible:underline focus-visible:outline-none"
                        style={nameColor !== null ? { color: nameColor } : undefined}
                      >
                        {name}
                      </button>
                      <span className="text-xs text-faint">
                        {formatTimestamp(m.sent_ms, lang)}
                      </span>
                      {m.acked === false && (
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
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}
