/** Vue conversation privée (MP), extraite de `ChatView` (D-056). */

import { useEffect, useState } from 'react';
import { interpolate } from '../../i18n';
import { useCalls } from '../../stores/calls';
import { useContextMenu } from '../../stores/contextMenu';
import { useDms } from '../../stores/dms';
import {
  useFriends,
  avatarDecorationOf,
  avatarOf,
  displayNameOf,
  presenceOf,
} from '../../stores/friends';
import { useGroups, aggregateEmojiMap } from '../../stores/groups';
import { selfDisplayName, useSession } from '../../stores/session';
import { dmTypingKey } from '../../stores/typing';
import { useUi, useT } from '../../stores/ui';
import { Avatar } from '../Avatar';
import { buildContactMenu } from '../contactMenu';
import { PhoneIcon } from '../ContextMenu';
import { EphemeralPicker } from '../EphemeralPicker';
import { MessageInput } from '../MessageInput';
import { MessageList, type DisplayMessage } from '../MessageList';
import { MessageListSkeleton } from '../Skeleton';
import { ScheduleMessageDialog } from '../ScheduleMessageDialog';
import { deliveryOf } from '../messageModel';
import { TypingIndicator } from '../TypingIndicator';
import {
  HeaderIconButton,
  ReplyBanner,
  mentionSet,
  resolvePins,
  useMessageJump,
} from './common';
import { PinnedPanel } from './panels';
import { copyConversation } from './copyConversation';

export function DmView({ peer }: { peer: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const callPhase = useCalls((s) => s.phase);
  const callPeer = useCalls((s) => s.peer);
  const startCall = useCalls((s) => s.start);
  const clearMissed = useCalls((s) => s.clearMissed);
  const messages = useDms((s) => s.conversations[peer]) ?? [];
  const hasMore = useDms((s) => s.hasMore[peer]) === true;
  const refresh = useDms((s) => s.refresh);
  const loadOlder = useDms((s) => s.loadOlder);
  const send = useDms((s) => s.send);
  const edit = useDms((s) => s.edit);
  const deleteMessage = useDms((s) => s.deleteMessage);
  const toggleReaction = useDms((s) => s.toggleReaction);
  const pins = useDms((s) => s.pins[peer]) ?? [];
  const loadPins = useDms((s) => s.loadPins);
  const togglePin = useDms((s) => s.togglePin);
  const retry = useDms((s) => s.retry);
  const markRead = useFriends((s) => s.markRead);
  const requestJump = useUi((s) => s.requestJump);
  // Serveurs rejoints par le membre local, pour l'agrégat d'émojis custom
  // (aucun `groupId` de contexte en MP — voir `emojiMap` ci-dessous).
  const groupIds = useGroups((s) => s.ids);
  const groupStates = useGroups((s) => s.states);
  const name = displayNameOf(contacts, peer);
  /**
   * Première page d'historique pas encore résolue : affiche le squelette tant
   * que le fil est vide et que le chargement initial est en cours (évite
   * l'éclair « aucun message » sur une conversation qui en a).
   */
  const [chargement, setChargement] = useState(true);
  /** Volet des messages épinglés (fermé par défaut). */
  const [pinsOpen, setPinsOpen] = useState(false);
  const [scheduleOpen, setScheduleOpen] = useState(false);
  /** Message auquel la prochaine saisie répondra (null : envoi simple). */
  const [replyTo, setReplyTo] = useState<DisplayMessage | null>(null);
  /**
   * Position lue figée à l'ouverture du MP (séparateur « nouveaux messages ») :
   * capturée AVANT que markRead ne la fasse avancer. `null` = pas de séparateur.
   */
  const [dividerLamport, setDividerLamport] = useState<number | null>(null);
  /** Lamport du dernier message affiché (`null` : fil vide). */
  const lastLamport = messages.at(-1)?.lamport ?? null;

  const scrollTarget = useMessageJump(
    (jump) => jump.view.kind === 'dm' && jump.view.peer === peer,
    (msgId) => useDms.getState().jumpTo(peer, msgId),
  );

  useEffect(() => {
    setPinsOpen(false);
    setReplyTo(null);
    setChargement(true);
    // Un saut vers cette conversation charge lui-même la fenêtre du message
    // ciblé : on évite alors le rechargement récent qui l'écraserait.
    const jump = useUi.getState().jump;
    const jumpingHere = jump?.view.kind === 'dm' && jump.view.peer === peer;
    if (jumpingHere) {
      setChargement(false);
    } else {
      refresh(peer)
        .catch(() => toast('error', t.errors.loadFailed))
        .finally(() => setChargement(false));
    }
    // Épinglés en best effort : le volet affichera ce qui est connu.
    loadPins(peer).catch(() => {});
  }, [peer, refresh, loadPins, toast, t]);

  // Conversation ouverte : efface le badge d'appel manqué de ce pair, le cas
  // échéant (voir `stores/calls.ts`).
  useEffect(() => {
    clearMissed(peer);
  }, [peer, clearMissed]);

  // Capture one-shot de la position lue à l'ouverture (dépend de `peer` seul) :
  // lue depuis le store AVANT le markRead ci-dessous, elle fige le séparateur.
  // markRead avance la marque via `friends.load`, mais cet effet ne se rejoue
  // pas (deps inchangées) — le séparateur reste à la position d'entrée.
  useEffect(() => {
    const c = useFriends.getState().contacts.find((x) => x.pubkey === peer);
    setDividerLamport(c?.read_lamport ?? null);
  }, [peer]);

  // Conversation affichée : marquée lue jusqu'au dernier message connu, à
  // l'ouverture comme à chaque arrivée — le badge de non-lus retombe. Seule
  // la conversation montrée est concernée (le composant est démonté sinon).
  useEffect(() => {
    if (lastLamport === null) return;
    markRead(peer, lastLamport).catch(() => {
      // Best effort : les compteurs seront corrigés au prochain passage.
    });
  }, [peer, lastLamport, markRead]);

  const onActionError = (): void => toast('error', t.errors.actionFailed);
  const knownMentions = mentionSet(contacts, self);
  const pinnedIds = new Set(pins);
  const nameOf = (author: string): string =>
    self && author === self.pubkey
      ? `${selfDisplayName(self)}`
      : displayNameOf(contacts, author);
  const { resolved: resolvedPins, unresolved: unresolvedPins } = resolvePins(
    pins,
    messages,
  );
  // Aucun serveur de contexte en MP : les émojis custom viennent de l'agrégat
  // de tous les serveurs rejoints (voir `aggregateEmojiMap`) — l'image reste
  // chargée par sa racine Merkle, indépendante du groupe. Repli en texte
  // `:name:` si l'émoji n'appartient à aucun serveur du destinataire.
  const emojiMap = aggregateEmojiMap(groupIds, groupStates);

  const ouvrirProfil = (target: HTMLElement): void => {
    const r = target.getBoundingClientRect();
    useUi.getState().openProfile(peer, {
      top: r.top,
      left: r.left,
      bottom: r.bottom,
      right: r.right,
    });
  };

  // Appel 1-à-1 : réservé aux amis confirmés (contrat calls.start) ; le
  // bouton est simplement absent sinon plutôt qu'un état désactivé confus.
  const isFriend = contacts.some((c) => c.pubkey === peer && c.state === 'friend');
  // Boîte chiffrée : pair hors ligne ET au moins un message local encore
  // `pending` ⇒ bandeau discret en tête de fil — les envois sont déposés dans
  // la boîte du destinataire (expiration 7 j côté nœud), pas perdus.
  const peerOffline = presenceOf(contacts.find((c) => c.pubkey === peer)) === 'offline';
  const hasPendingLocal =
    self !== null &&
    messages.some((m) => m.author === self.pubkey && deliveryOf(m) === 'pending');
  const callOngoing = callPhase !== 'idle';
  const inCallWithPeer = callPhase === 'active' && callPeer === peer;
  const onStartCall = (): void => {
    if (callOngoing) return;
    startCall(peer).catch(onActionError);
  };

  return (
    <div className="accord-conversation relative flex h-full flex-col">
      <header className="accord-chat-header flex h-12 shrink-0 items-center gap-3 border-b border-[color:var(--glass-border)] bg-chat/90 px-4 shadow-1">
        <button
          type="button"
          aria-label={interpolate(t.profil.openProfile, { name })}
          onClick={(e) => ouvrirProfil(e.currentTarget)}
          onContextMenu={(e) => {
            const contact = contacts.find((c) => c.pubkey === peer);
            if (contact === undefined) return;
            e.preventDefault();
            useContextMenu
              .getState()
              .openMenu(
                e.clientX,
                e.clientY,
                buildContactMenu(t, contact, e.currentTarget),
              );
          }}
          className="flex min-w-0 flex-1 items-center gap-3 rounded-md px-1 py-0.5 text-left transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
        >
          <Avatar
            id={peer}
            name={name}
            size={26}
            avatarHash={avatarOf(contacts, peer)}
            hint={peer}
            decoration={avatarDecorationOf(contacts, peer)}
            online={
              contacts.some((c) => c.pubkey === peer && c.state === 'friend')
                ? (contacts.find((c) => c.pubkey === peer)?.online ?? false)
                : undefined
            }
          />
          <span className="min-w-0 truncate font-semibold text-header" title={name}>
            {name}
          </span>
          {inCallWithPeer && (
            <span className="rounded-full bg-green/15 px-2 py-0.5 text-xs font-medium text-green">
              {t.calls.inCall}
            </span>
          )}
        </button>
        <div className="ml-auto flex shrink-0 items-center gap-1">
          <EphemeralPicker scope={{ kind: 'dm', peer }} variant="header" />
          <HeaderIconButton
            label={t.planning.scheduleOpen}
            active={scheduleOpen}
            onClick={() => setScheduleOpen(true)}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <circle cx="12" cy="12" r="9" />
              <path d="M12 7v5l3 2" />
            </svg>
          </HeaderIconButton>
          {isFriend && (
            <HeaderIconButton
              label={callOngoing ? t.calls.callAlreadyOngoing : t.calls.startCall}
              active={false}
              disabled={callOngoing}
              onClick={onStartCall}
            >
              <PhoneIcon />
            </HeaderIconButton>
          )}
          <HeaderIconButton
            label={t.transcript.copyConversation}
            active={false}
            onClick={() => copyConversation(messages, nameOf, name, t)}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" />
              <rect x="8" y="2" width="8" height="4" rx="1" />
              <path d="M8 11h8M8 15h5" />
            </svg>
          </HeaderIconButton>
          <HeaderIconButton
            label={t.serveur.pinnedTitle}
            active={pinsOpen}
            ariaExpanded={pinsOpen}
            onClick={() => setPinsOpen((open) => !open)}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <line x1="12" x2="12" y1="17" y2="22" />
              <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z" />
            </svg>
          </HeaderIconButton>
        </div>
      </header>
      {pinsOpen && (
        <PinnedPanel
          resolved={resolvedPins}
          unresolved={unresolvedPins}
          canUnpin
          onUnpin={(msgId) => {
            togglePin(peer, msgId, true).catch(onActionError);
          }}
          onJump={(msgId) => {
            setPinsOpen(false);
            requestJump({ kind: 'dm', peer }, msgId);
          }}
          onClose={() => setPinsOpen(false)}
          nameOf={nameOf}
        />
      )}
      {scheduleOpen && (
        <ScheduleMessageDialog peer={peer} onClose={() => setScheduleOpen(false)} />
      )}
      {peerOffline && hasPendingLocal && (
        <div className="px-4 pt-2">
          <div
            role="status"
            className="flex items-center gap-2.5 rounded-xl bg-input px-4 py-2.5 text-sm text-muted"
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
              className="shrink-0"
            >
              <path d="M22 17a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V9.5C2 7 4 5 6.5 5H18a4 4 0 0 1 4 4Z" />
              <polyline points="15,9 18,9 18,11" />
              <path d="M6.5 5C9 5 11 7 11 9.5V17a2 2 0 0 1-2 2" />
              <line x1="6" x2="7" y1="10" y2="10" />
            </svg>
            <span>{interpolate(t.dm.mailboxBanner, { name })}</span>
          </div>
        </div>
      )}
      {chargement && messages.length === 0 ? (
        <MessageListSkeleton label={t.app.loading} />
      ) : (
        <MessageList
          key={peer}
          messages={messages}
          hasMore={hasMore}
          scrollTarget={scrollTarget}
          dividerLamport={dividerLamport}
          pinnedIds={pinnedIds}
          knownMentions={knownMentions}
          emojiMap={emojiMap}
          onLoadOlder={() => {
            loadOlder(peer).catch(() => toast('error', t.errors.loadFailed));
          }}
          actions={{
            onReact: (message, emoji) => {
              if (!self) return;
              toggleReaction(peer, message.msg_id, emoji, self.pubkey).catch(
                onActionError,
              );
            },
            onReply: (message) => setReplyTo(message),
            onEdit: (message, text) => {
              edit(peer, message.msg_id, text).catch(onActionError);
            },
            onDelete: (message) => {
              deleteMessage(peer, message.msg_id).catch(onActionError);
            },
            onTogglePin: (message, pinned) => {
              togglePin(peer, message.msg_id, pinned).catch(onActionError);
            },
            onRetry: (message) => {
              retry(peer, message.msg_id).catch(onActionError);
            },
          }}
        />
      )}
      {replyTo !== null && (
        <ReplyBanner
          name={
            self && replyTo.author === self.pubkey
              ? `${selfDisplayName(self)}`
              : displayNameOf(contacts, replyTo.author)
          }
          onCancel={() => setReplyTo(null)}
        />
      )}
      <TypingIndicator typingKey={dmTypingKey(peer)} />
      <MessageInput
        placeholder={interpolate(t.dm.placeholder, { name })}
        typingTarget={{ kind: 'dm', peer }}
        focusKey={replyTo?.msg_id ?? null}
        onSend={async (text, attachments) => {
          await send(peer, text, replyTo?.msg_id, attachments);
          setReplyTo(null);
        }}
      />
    </div>
  );
}
