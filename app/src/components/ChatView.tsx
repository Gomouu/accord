/** Vue conversation : MP (deux personnes) ou salon de groupe + membres. */

import { useEffect, useState } from 'react';
import { interpolate } from '../i18n';
import type { Contact, SelfProfile } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
import { formatTimestamp } from '../lib/format';
import { useContextMenu, type ContextMenuItem } from '../stores/contextMenu';
import { useDms } from '../stores/dms';
import { useFriends, avatarOf, displayNameOf } from '../stores/friends';
import {
  useGroups,
  aggregateEmojiMap,
  channelKey,
  hasPerm,
  memberColor,
  nicknameOf,
  sortRoles,
  PERMISSIONS,
} from '../stores/groups';
import { selfDisplayName, useSession } from '../stores/session';
import { dmTypingKey, groupTypingKey } from '../stores/typing';
import { useUi, useT, type JumpRequest } from '../stores/ui';
import { Avatar } from './Avatar';
import { CopyMenuIcon, EnvelopeMenuIcon, MentionMenuIcon, ProfileMenuIcon } from './ContextMenu';
import { MessageInput } from './MessageInput';
import { MessageList, type DisplayMessage } from './MessageList';
import { TypingIndicator } from './TypingIndicator';

/**
 * Traite la demande de saut de l'UI qui vise la vue courante : charge la
 * fenêtre du message (via `load`) puis rend la cible à révéler (défilement +
 * surbrillance dans `MessageList`). Une cible absente déclenche un toast.
 */
function useMessageJump(
  matches: (jump: JumpRequest) => boolean,
  load: (msgId: string) => Promise<boolean>,
): { msgId: string; nonce: number } | null {
  const jump = useUi((s) => s.jump);
  const clearJump = useUi((s) => s.clearJump);
  const toast = useUi((s) => s.toast);
  const t = useT();
  const [scrollTarget, setScrollTarget] = useState<{
    msgId: string;
    nonce: number;
  } | null>(null);

  useEffect(() => {
    if (jump === null || !matches(jump)) return;
    const req = jump;
    let cancelled = false;
    void (async () => {
      let found = false;
      try {
        found = await load(req.msgId);
      } catch {
        found = false;
      }
      if (cancelled) return;
      if (found) setScrollTarget({ msgId: req.msgId, nonce: req.nonce });
      else toast('error', t.dm.messageUnavailable);
      clearJump();
    })();
    return () => {
      cancelled = true;
    };
    // `matches`/`load` capturent la vue courante ; seul un nouveau saut relance.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jump]);

  return scrollTarget;
}

/** Messages épinglés résolus dans l'historique chargé + nombre hors-page. */
function resolvePins(
  pinnedIds: readonly string[],
  messages: readonly DisplayMessage[],
): { resolved: DisplayMessage[]; unresolved: number } {
  const byId = new Map(messages.map((m) => [m.msg_id, m]));
  const resolved = pinnedIds.flatMap((id) => {
    const message = byId.get(id);
    return message !== undefined && !message.deleted ? [message] : [];
  });
  return { resolved, unresolved: pinnedIds.length - resolved.length };
}

/** Noms (en minuscules) reconnus comme mentions : contacts nommés + soi-même. */
function mentionSet(contacts: Contact[], self: SelfProfile | null): Set<string> {
  const noms = new Set<string>();
  for (const c of contacts) {
    if (c.display_name.trim() !== '') noms.add(c.display_name.toLowerCase());
  }
  if (self !== null) noms.add(selfDisplayName(self).toLowerCase());
  return noms;
}

/** Bandeau « Répondre à … » au-dessus de la zone de saisie, annulable. */
function ReplyBanner({ name, onCancel }: { name: string; onCancel: () => void }) {
  const t = useT();
  // Le nom est mis en gras quelle que soit sa position dans le libellé.
  const [before, after] = t.dm.replyingTo.split('{name}');
  return (
    <div className="mx-4 -mb-1 flex items-center justify-between rounded-t-lg bg-sidebar px-4 pb-2.5 pt-1.5 text-sm">
      <span className="min-w-0 truncate text-muted">
        {before}
        <span className="font-semibold text-header">{name}</span>
        {after}
      </span>
      <button
        type="button"
        aria-label={t.dm.cancelReply}
        title={t.dm.cancelReply}
        onClick={onCancel}
        className="ml-2 shrink-0 rounded-full text-faint transition-colors duration-fast hover:text-norm focus-visible:text-norm focus-visible:outline-none active:scale-90"
      >
        <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
          <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm3.6 12.2-1.4 1.4L12 13.4l-2.2 2.2-1.4-1.4 2.2-2.2-2.2-2.2 1.4-1.4 2.2 2.2 2.2-2.2 1.4 1.4-2.2 2.2 2.2 2.2Z" />
        </svg>
      </button>
    </div>
  );
}

export function DmView({ peer }: { peer: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
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
  /** Volet des messages épinglés (fermé par défaut). */
  const [pinsOpen, setPinsOpen] = useState(false);
  /** Message auquel la prochaine saisie répondra (null : envoi simple). */
  const [replyTo, setReplyTo] = useState<DisplayMessage | null>(null);
  /** Lamport du dernier message affiché (`null` : fil vide). */
  const lastLamport = messages.at(-1)?.lamport ?? null;

  const scrollTarget = useMessageJump(
    (jump) => jump.view.kind === 'dm' && jump.view.peer === peer,
    (msgId) => useDms.getState().jumpTo(peer, msgId),
  );

  useEffect(() => {
    setPinsOpen(false);
    setReplyTo(null);
    // Un saut vers cette conversation charge lui-même la fenêtre du message
    // ciblé : on évite alors le rechargement récent qui l'écraserait.
    const jump = useUi.getState().jump;
    const jumpingHere = jump?.view.kind === 'dm' && jump.view.peer === peer;
    if (!jumpingHere) {
      refresh(peer).catch(() => toast('error', t.errors.loadFailed));
    }
    loadPins(peer).catch(() => {});
  }, [peer, refresh, loadPins, toast, t]);

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
      ? `${selfDisplayName(self)} (${t.app.you})`
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

  return (
    <div className="relative flex h-full flex-col">
      <header className="flex h-12 items-center gap-3 border-b border-rail px-4 shadow-sm">
        <button
          type="button"
          aria-label={interpolate(t.profil.openProfile, { name })}
          onClick={(e) => ouvrirProfil(e.currentTarget)}
          className="flex items-center gap-3 rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        >
          <Avatar
            id={peer}
            name={name}
            size={26}
            avatarHash={avatarOf(contacts, peer)}
            hint={peer}
            online={
              contacts.some((c) => c.pubkey === peer && c.state === 'friend')
                ? (contacts.find((c) => c.pubkey === peer)?.online ?? false)
                : undefined
            }
          />
          <span className="font-semibold text-header">{name}</span>
        </button>
        <div className="ml-auto">
          <button
            type="button"
            aria-label={t.serveur.pinnedTitle}
            title={t.serveur.pinnedTitle}
            aria-expanded={pinsOpen}
            onClick={() => setPinsOpen((open) => !open)}
            className={`rounded p-1.5 transition-colors duration-fast hover:bg-chat-hover active:scale-95 ${
              pinsOpen ? 'text-header' : 'text-muted hover:text-norm'
            }`}
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
              <path d="M14.6 2.6a1 1 0 0 1 1.4 0l5.4 5.4a1 1 0 0 1 0 1.4l-1.2 1.2a1 1 0 0 1-1 .3l-.7-.2-3.7 3.7.4 2.7a1 1 0 0 1-.3.9l-.9.9a1 1 0 0 1-1.4 0l-3.2-3.2-4.7 4.7a1 1 0 0 1-1.5-1.5l4.8-4.7-3.3-3.2a1 1 0 0 1 0-1.4l1-.9a1 1 0 0 1 .8-.3l2.7.4 3.7-3.7-.2-.7a1 1 0 0 1 .3-1l1.6-.8Z" />
            </svg>
          </button>
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
      <MessageList
        key={peer}
        messages={messages}
        hasMore={hasMore}
        scrollTarget={scrollTarget}
        pinnedIds={pinnedIds}
        knownMentions={knownMentions}
        emojiMap={emojiMap}
        onLoadOlder={() => {
          loadOlder(peer).catch(() => toast('error', t.errors.loadFailed));
        }}
        actions={{
          onReact: (message, emoji) => {
            if (!self) return;
            toggleReaction(peer, message.msg_id, emoji, self.pubkey).catch(onActionError);
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
      {replyTo !== null && (
        <ReplyBanner
          name={
            self && replyTo.author === self.pubkey
              ? `${selfDisplayName(self)} (${t.app.you})`
              : displayNameOf(contacts, replyTo.author)
          }
          onCancel={() => setReplyTo(null)}
        />
      )}
      <MessageInput
        placeholder={interpolate(t.dm.placeholder, { name })}
        typingTarget={{ kind: 'dm', peer }}
        onSend={async (text, attachments) => {
          await send(peer, text, replyTo?.msg_id, attachments);
          setReplyTo(null);
        }}
      />
      <TypingIndicator typingKey={dmTypingKey(peer)} />
    </div>
  );
}

function MemberList({ groupId }: { groupId: string }) {
  const t = useT();
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const state = useGroups((s) => s.states[groupId]);
  const openProfile = useUi((s) => s.openProfile);
  const requestMentionInsert = useUi((s) => s.requestMentionInsert);
  const toast = useUi((s) => s.toast);
  if (!state) return null;

  const nameOf = (pubkey: string): string => {
    const nick = nicknameOf(state, pubkey);
    if (self && pubkey === self.pubkey) {
      return `${nick ?? selfDisplayName(self)} (${t.app.you})`;
    }
    return nick ?? displayNameOf(contacts, pubkey);
  };

  /**
   * Items du menu contextuel d'une entrée de la liste des membres : profil,
   * mention, MP, copie d'identifiant — même schéma que le menu « utilisateur »
   * de `MessageList` (avatar/pseudo d'un message).
   */
  const buildMemberItems = (pubkey: string, target: HTMLElement): ContextMenuItem[] => {
    const isSelfMember = self !== null && pubkey === self.pubkey;
    const contact = contacts.find((c) => c.pubkey === pubkey);
    const canMessage = !isSelfMember && contact?.state === 'friend';
    const items: ContextMenuItem[] = [
      {
        label: t.contextMenu.viewProfile,
        icon: <ProfileMenuIcon />,
        onClick: () => {
          const r = target.getBoundingClientRect();
          openProfile(
            pubkey,
            { top: r.top, left: r.left, bottom: r.bottom, right: r.right },
            groupId,
          );
        },
      },
      {
        label: interpolate(t.contextMenu.mention, { name: nameOf(pubkey) }),
        icon: <MentionMenuIcon />,
        onClick: () => requestMentionInsert(nameOf(pubkey)),
      },
    ];
    if (canMessage) {
      items.push({
        label: t.friends.sendDm,
        icon: <EnvelopeMenuIcon />,
        onClick: () => useUi.getState().setView({ kind: 'dm', peer: pubkey }),
      });
    }
    items.push({
      label: t.contextMenu.copyUserId,
      icon: <CopyMenuIcon />,
      separatorBefore: true,
      onClick: () =>
        copyToClipboard(
          pubkey,
          () => toast('info', t.app.copied),
          () => toast('error', t.errors.actionFailed),
        ),
    });
    return items;
  };

  /** Noms des rôles d'un membre, du plus haut au plus bas. */
  const roleNamesOf = (roleIds: readonly string[]): string[] => {
    const owned = new Set(roleIds);
    return sortRoles(state.roles)
      .filter((r) => owned.has(r.role_id))
      .map((r) => r.name);
  };

  return (
    <aside className="w-60 overflow-y-auto bg-sidebar p-3" aria-label={t.groups.members}>
      <div className="px-1 pb-2 text-xs font-semibold uppercase tracking-wide text-faint">
        {t.groups.members} — {state.members.length}
      </div>
      {state.members.map((member) => {
        const color = memberColor(member, state.roles);
        const roleNames = roleNamesOf(member.roles);
        // Avatar d'un membre : le sien, sinon celui du contact ami connu —
        // les avatars des non-amis ne circulent pas (limite du protocole).
        const avatarHash =
          self && member.pubkey === self.pubkey
            ? self.avatar
            : avatarOf(contacts, member.pubkey);
        return (
          <button
            key={member.pubkey}
            type="button"
            aria-label={interpolate(t.profil.openProfile, {
              name: nameOf(member.pubkey),
            })}
            onClick={(e) => {
              const r = e.currentTarget.getBoundingClientRect();
              openProfile(
                member.pubkey,
                { top: r.top, left: r.left, bottom: r.bottom, right: r.right },
                groupId,
              );
            }}
            onContextMenu={(e) => {
              e.preventDefault();
              useContextMenu
                .getState()
                .openMenu(e.clientX, e.clientY, buildMemberItems(member.pubkey, e.currentTarget));
            }}
            className="flex w-full items-center gap-2.5 rounded px-1.5 py-1.5 text-left transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
          >
            <Avatar
              id={member.pubkey}
              name={nameOf(member.pubkey)}
              size={32}
              avatarHash={avatarHash}
              hint={member.pubkey}
            />
            <div className="min-w-0">
              <div
                className="truncate text-sm font-medium text-muted"
                style={color !== null ? { color } : undefined}
              >
                {nameOf(member.pubkey)}
              </div>
              {state.founder === member.pubkey && (
                <div className="text-[10px] uppercase text-yellow">
                  {t.groups.founder}
                </div>
              )}
              {roleNames.length > 0 && (
                <div className="truncate text-[11px] text-faint">
                  {roleNames.join(' · ')}
                </div>
              )}
            </div>
          </button>
        );
      })}
    </aside>
  );
}

/**
 * Volet des messages épinglés (MP ou salon) : messages déjà résolus dans
 * l'historique chargé + nombre d'épinglés hors-page. Chaque entrée saute vers
 * le message d'un clic ; `canUnpin` expose le retrait de l'épingle.
 */
function PinnedPanel({
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

  return (
    <div
      role="dialog"
      aria-label={t.serveur.pinnedTitle}
      className="absolute right-4 top-14 z-20 max-h-96 w-96 max-w-[85vw] overflow-y-auto rounded-lg border border-rail bg-modal p-3 shadow-elevation"
    >
      <div className="flex items-center justify-between pb-2">
        <span className="text-sm font-semibold text-header">{t.serveur.pinnedTitle}</span>
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
      {resolved.length === 0 && unresolved === 0 && (
        <p className="py-3 text-center text-sm text-muted">{t.serveur.noPins}</p>
      )}
      {resolved.map((m) => (
        <div key={m.msg_id} className="group/pin mb-1 rounded bg-sidebar px-3 py-2">
          <button
            type="button"
            onClick={() => onJump(m.msg_id)}
            className="block w-full text-left"
          >
            <div className="flex items-baseline justify-between gap-2">
              <span className="truncate text-sm font-medium text-header">
                {nameOf(m.author)}
              </span>
              <span className="shrink-0 text-xs text-faint">
                {formatTimestamp(m.sent_ms, lang)}
              </span>
            </div>
            <div className="break-words text-sm text-norm">
              {m.edited ?? (m.body.type === 'text' ? m.body.text : t.dm.unsupported)}
            </div>
          </button>
          {canUnpin && (
            <button
              type="button"
              onClick={() => onUnpin(m.msg_id)}
              className="mt-1 text-xs font-medium text-muted transition-colors duration-fast hover:text-red"
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

export function GroupView({
  groupId,
  channelId,
}: {
  groupId: string;
  channelId: string | null;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const self = useSession((s) => s.self);
  const contacts = useFriends((s) => s.contacts);
  const state = useGroups((s) => s.states[groupId]);
  const key = channelId === null ? null : channelKey(groupId, channelId);
  const messages = useGroups((s) => (key === null ? undefined : s.messages[key])) ?? [];
  const hasMore = useGroups((s) => (key === null ? false : s.hasMore[key])) === true;
  const pins = useGroups((s) => (key === null ? undefined : s.pins[key]));
  const refreshHistory = useGroups((s) => s.refreshHistory);
  const loadOlderHistory = useGroups((s) => s.loadOlderHistory);
  const loadPins = useGroups((s) => s.loadPins);
  const send = useGroups((s) => s.send);
  const editMessage = useGroups((s) => s.editMessage);
  const deleteMessage = useGroups((s) => s.deleteMessage);
  const toggleReaction = useGroups((s) => s.toggleReaction);
  const togglePin = useGroups((s) => s.togglePin);
  const markRead = useGroups((s) => s.markRead);
  const requestJump = useUi((s) => s.requestJump);
  /** Volet des messages épinglés (fermé par défaut). */
  const [pinsOpen, setPinsOpen] = useState(false);
  /** Message auquel la prochaine saisie répondra (null : envoi simple). */
  const [replyTo, setReplyTo] = useState<DisplayMessage | null>(null);
  /** Lamport du dernier message affiché (`null` : fil vide). */
  const lastLamport = messages.at(-1)?.lamport ?? null;

  const scrollTarget = useMessageJump(
    (jump) =>
      jump.view.kind === 'group' &&
      jump.view.groupId === groupId &&
      jump.view.channelId === channelId,
    (msgId) =>
      channelId === null
        ? Promise.resolve(false)
        : useGroups.getState().jumpTo(groupId, channelId, msgId),
  );

  const channel = state?.channels.find((c) => c.channel_id === channelId);
  const canModerate = hasPerm(state?.my_permissions ?? 0, PERMISSIONS.MANAGE_MESSAGES);

  useEffect(() => {
    setPinsOpen(false);
    setReplyTo(null);
    if (channelId !== null) {
      // Un saut vers ce salon charge lui-même la fenêtre du message ciblé :
      // on évite alors le rechargement récent qui l'écraserait.
      const jump = useUi.getState().jump;
      const jumpingHere =
        jump?.view.kind === 'group' &&
        jump.view.groupId === groupId &&
        jump.view.channelId === channelId;
      if (!jumpingHere) {
        refreshHistory(groupId, channelId).catch(() =>
          toast('error', t.errors.loadFailed),
        );
      }
      // Épinglés en best effort : le volet affichera ce qui est connu.
      loadPins(groupId, channelId).catch(() => {});
    }
  }, [groupId, channelId, refreshHistory, loadPins, toast, t]);

  // Salon affiché : marqué lu jusqu'au dernier message connu, à l'ouverture
  // comme à chaque arrivée — le badge de non-lus du salon retombe.
  useEffect(() => {
    if (channelId === null || lastLamport === null) return;
    markRead(groupId, channelId, lastLamport).catch(() => {
      // Best effort : les compteurs seront corrigés au prochain passage.
    });
  }, [groupId, channelId, lastLamport, markRead]);

  if (channelId === null || !channel) {
    return (
      <div className="flex h-full items-center justify-center text-muted">
        {t.groups.noChannel}
      </div>
    );
  }

  const onActionError = (): void => toast('error', t.errors.actionFailed);
  const pinnedIds = new Set(pins ?? []);
  const colorOf = (author: string): string | null =>
    state === undefined
      ? null
      : memberColor(
          state.members.find((m) => m.pubkey === author),
          state.roles,
        );
  const emojiMap = new Map(
    (state?.emojis ?? []).map((e) => [e.name, e.merkle_root] as const),
  );
  const knownMentions = mentionSet(contacts, self);
  for (const m of state?.members ?? []) {
    knownMentions.add(displayNameOf(contacts, m.pubkey).toLowerCase());
  }
  const nameOf = (author: string): string => {
    const nick = nicknameOf(state, author);
    if (self && author === self.pubkey) {
      return `${nick ?? selfDisplayName(self)} (${t.app.you})`;
    }
    return nick ?? displayNameOf(contacts, author);
  };
  const { resolved: resolvedPins, unresolved: unresolvedPins } = resolvePins(
    pins ?? [],
    messages,
  );

  return (
    <div className="flex h-full">
      <div className="relative flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 items-center gap-2 border-b border-rail px-4 shadow-sm">
          <span aria-hidden className="text-2xl font-light text-faint">
            #
          </span>
          <span className="shrink-0 font-semibold text-header">{channel.name}</span>
          {channel.topic !== '' && (
            <>
              <span aria-hidden className="h-5 w-px shrink-0 bg-input" />
              <span className="min-w-0 truncate text-sm text-muted" title={channel.topic}>
                {channel.topic}
              </span>
            </>
          )}
          <div className="ml-auto">
            <button
              type="button"
              aria-label={t.serveur.pinnedTitle}
              title={t.serveur.pinnedTitle}
              aria-expanded={pinsOpen}
              onClick={() => setPinsOpen((open) => !open)}
              className={`rounded p-1.5 transition-colors duration-fast hover:bg-chat-hover active:scale-95 ${
                pinsOpen ? 'text-header' : 'text-muted hover:text-norm'
              }`}
            >
              <svg
                width="18"
                height="18"
                viewBox="0 0 24 24"
                fill="currentColor"
                aria-hidden
              >
                <path d="M14.6 2.6a1 1 0 0 1 1.4 0l5.4 5.4a1 1 0 0 1 0 1.4l-1.2 1.2a1 1 0 0 1-1 .3l-.7-.2-3.7 3.7.4 2.7a1 1 0 0 1-.3.9l-.9.9a1 1 0 0 1-1.4 0l-3.2-3.2-4.7 4.7a1 1 0 0 1-1.5-1.5l4.8-4.7-3.3-3.2a1 1 0 0 1 0-1.4l1-.9a1 1 0 0 1 .8-.3l2.7.4 3.7-3.7-.2-.7a1 1 0 0 1 .3-1l1.6-.8Z" />
              </svg>
            </button>
          </div>
        </header>
        {pinsOpen && (
          <PinnedPanel
            resolved={resolvedPins}
            unresolved={unresolvedPins}
            canUnpin={canModerate}
            onUnpin={(msgId) => {
              togglePin(groupId, channelId, msgId, true).catch(onActionError);
            }}
            onJump={(msgId) => {
              setPinsOpen(false);
              requestJump(
                { kind: 'group', groupId, channelId },
                msgId,
              );
            }}
            onClose={() => setPinsOpen(false)}
            nameOf={nameOf}
          />
        )}
        <MessageList
          key={key ?? undefined}
          messages={messages}
          hasMore={hasMore}
          scrollTarget={scrollTarget}
          onLoadOlder={() => {
            loadOlderHistory(groupId, channelId).catch(() =>
              toast('error', t.errors.loadFailed),
            );
          }}
          actions={{
            onReact: (message, emoji) => {
              if (!self) return;
              toggleReaction(
                groupId,
                channelId,
                message.msg_id,
                emoji,
                self.pubkey,
              ).catch(onActionError);
            },
            onReply: (message) => setReplyTo(message),
            onEdit: (message, text) => {
              editMessage(groupId, channelId, message.msg_id, text).catch(onActionError);
            },
            onDelete: (message) => {
              deleteMessage(groupId, channelId, message.msg_id).catch(onActionError);
            },
            canModerate,
            ...(canModerate
              ? {
                  onTogglePin: (message: DisplayMessage, pinned: boolean) => {
                    togglePin(groupId, channelId, message.msg_id, pinned).catch(
                      onActionError,
                    );
                  },
                }
              : {}),
          }}
          pinnedIds={pinnedIds}
          colorOf={colorOf}
          emojiMap={emojiMap}
          knownMentions={knownMentions}
          groupId={groupId}
        />
        {replyTo !== null && (
          <ReplyBanner
            name={
              self && replyTo.author === self.pubkey
                ? `${selfDisplayName(self)} (${t.app.you})`
                : displayNameOf(contacts, replyTo.author)
            }
            onCancel={() => setReplyTo(null)}
          />
        )}
        <MessageInput
          placeholder={interpolate(t.groups.channelPlaceholder, { name: channel.name })}
          groupId={groupId}
          typingTarget={{ kind: 'group', groupId, channelId }}
          onSend={async (text, attachments) => {
            await send(groupId, channelId, text, replyTo?.msg_id, attachments);
            setReplyTo(null);
          }}
        />
        <TypingIndicator typingKey={groupTypingKey(groupId, channelId)} />
      </div>
      <MemberList groupId={groupId} />
    </div>
  );
}
