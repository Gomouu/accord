/** Coquille principale trois colonnes : rail, barre latérale, contenu. */

import { useCallback, useEffect, useRef } from 'react';
import { dictionaries, interpolate } from '../i18n';
import type { AccordEvent, CallEndedReason } from '../lib/api';
import { callEndedToast } from '../lib/callToast';
import { setAppBadge } from '../lib/bridge';
import { rpc } from '../lib/client';
import { eventStartedToast } from '../lib/eventToast';
import {
  isNotificationEligible,
  isSoundEligible,
  isWithinQuietHours,
  rememberNotifiedConversation,
  sendNativeNotification,
  takePendingConversation,
  unreadBadgeTotal,
  type ConversationRef,
} from '../lib/notifications';
import { playNotificationSound } from '../lib/notificationSound';
import { usePushToTalk } from '../hooks/usePushToTalk';
import { cycleChannel, cycleDm, visibleNavigableChannels } from '../lib/quickSwitch';
import { useCalls } from '../stores/calls';
import { isEditableTarget } from '../stores/contextMenu';
import { useDms } from '../stores/dms';
import { useFriends, displayNameOf, totalDmUnread } from '../stores/friends';
import { useGroups, channelKey } from '../stores/groups';
import { isConversationSilenced } from '../stores/mute';
import { useSession } from '../stores/session';
import { useTyping, dmTypingKey, groupTypingKey } from '../stores/typing';
import {
  useUi,
  useT,
  type View,
  SIDEBAR_WIDTH_DEFAULT,
  SIDEBAR_WIDTH_MIN,
  SIDEBAR_WIDTH_MAX,
} from '../stores/ui';
import { useVoice } from '../stores/voice';
import { DmView, GroupView } from './ChatView';
import { ContextMenu } from './ContextMenu';
import { ErrorBoundary } from './ErrorBoundary';
import { FriendsView } from './FriendsView';
import { IncomingCall } from './IncomingCall';
import { Modals } from './Modals';
import { ProfilePopover } from './ProfilePopover';
import { QuickSwitcher } from './QuickSwitcher';
import { ResizeHandle } from './ResizeHandle';
import { ServerRail } from './ServerRail';
import { Sidebar } from './Sidebar';
import { ThemeAtmosphere } from './ThemeAtmosphere';

/**
 * Notification native « Nouveau message de <nom> » si les réglages
 * l'autorisent — jamais le contenu du message, jamais ses propres messages,
 * jamais si le niveau de notification du serveur/salon visé le tait
 * (`stores/mute.ts` : niveau 'none', ou 'mentions' sans mention). `mentionsMe`
 * porte le drapeau `mentions_me` du message (sans effet pour un MP, toujours
 * « pour moi »).
 */
function notifyNewMessage(
  ref: ConversationRef,
  author: string,
  mentionsMe: boolean,
): void {
  const self = useSession.getState().self;
  if (self === null) return;
  if (dndActif()) return;
  const ui = useUi.getState();
  const windowFocused = document.hasFocus();
  const eligible = isNotificationEligible({
    kind: ref.kind,
    prefs: {
      dms: ui.notifyDms,
      groups: ui.notifyGroups,
      onlyWhenUnfocused: ui.notifyOnlyUnfocused,
    },
    windowFocused,
    isOwnMessage: author === self.pubkey,
    muted: isConversationSilenced(ref, mentionsMe),
  });
  if (!eligible) return;
  const dict = dictionaries[ui.lang];
  const name = displayNameOf(useFriends.getState().contacts, author);
  void sendNativeNotification(
    dict.app.name,
    interpolate(dict.notifications.newMessageFrom, { name }),
  ).then((sent) => {
    // Notification shown while the window was unfocused: arm navigation on
    // the next focus, so clicking it (which activates the window on macOS
    // and Windows) opens the conversation — see lib/notifications.ts for the
    // per-platform behaviour of the Tauri notification plugin.
    if (sent && !windowFocused) rememberNotifiedConversation(ref);
  });
}

/** Vrai si `ref` désigne la conversation/le salon actuellement affiché. */
function isViewingConversation(view: View, ref: ConversationRef): boolean {
  if (ref.kind === 'dm') return view.kind === 'dm' && view.peer === ref.peer;
  return (
    view.kind === 'group' &&
    view.groupId === ref.groupId &&
    view.channelId === ref.channelId
  );
}

/**
 * Son in-app pour un message entrant : joue sauf pour ses propres messages,
 * en mode Ne pas déranger (statut de présence local, `useFriends.ownStatus`),
 * ou quand la fenêtre a le focus sur exactement cette conversation/ce salon
 * (la pastille de non-lu suffit alors — voir lib/notifications.ts). Une
 * mention (`mentions_me`) joue le blip renforcé. Respecte aussi le filtrage
 * choisi par l'utilisateur (Paramètres → Notifications → « Jouer un son
 * pour ») — le master `notifySoundEnabled` est vérifié plus bas, dans
 * `playNotificationSound` lui-même — et la sourdine locale du serveur/salon
 * (`stores/mute.ts`, menus contextuels de `ServerRail`/`Sidebar`).
 */
/** Ne pas déranger EFFECTIF : statut de présence Dnd OU heures calmes actives
 * (Paramètres → Notifications → Ne pas déranger programmé). */
function dndActif(): boolean {
  return (
    useFriends.getState().ownStatus === 'dnd' ||
    isWithinQuietHours(useUi.getState().quietHours, new Date())
  );
}

function maybePlaySound(ref: ConversationRef, author: string, isMention: boolean): void {
  const self = useSession.getState().self;
  if (self === null) return;
  const eligible = isSoundEligible({
    isOwnMessage: author === self.pubkey,
    isDisplayedConversation: isViewingConversation(useUi.getState().view, ref),
    windowFocused: document.hasFocus(),
    dnd: dndActif(),
    mode: useUi.getState().notifySoundMode,
    isMention,
    muted: isConversationSilenced(ref, isMention),
  });
  if (!eligible) return;
  playNotificationSound(isMention ? 'mention' : 'message');
}

/**
 * Alerte discrète pour une invitation de serveur entrante (consentement
 * explicite, D-045) : réutilise le blip de message existant (jamais en Ne
 * pas déranger, comme pour les messages) — la pastille elle-même vient
 * réactivement du store (`pendingInvites.length`), affichée dans la vue Amis.
 */
function maybePlayInviteSound(): void {
  if (dndActif()) return;
  playNotificationSound('message');
}

/**
 * Alerte discrète pour un événement de serveur qui démarre
 * (`event.group_event_started`) : toast + notification native (mêmes
 * réglages que les messages/invitations, jamais en Ne pas déranger), jamais
 * de son de mention (ce n'est pas une mention personnelle).
 */
function notifyEventStarted(title: string): void {
  const ui = useUi.getState();
  const dict = dictionaries[ui.lang];
  const toast = eventStartedToast(dict, title);
  ui.toast(toast.kind, toast.text);
  if (dndActif()) return;
  playNotificationSound('message');
  void sendNativeNotification(dict.app.name, toast.text);
}

/**
 * Traite `event.call_ended` : applique la transition (idle) au store d'appel
 * puis, seulement si elle a bien eu lieu (pas un événement tardif déjà résolu
 * localement par `decline`/`hangup` — voir `stores/calls.ts`), notifie
 * (`missed` marque aussi un badge sur le contact) et resynchronise le salon
 * vocal (la session d'appel se termine avec le moteur vocal existant).
 */
function handleCallEnded(params: {
  peer: string;
  call_id: string;
  reason: CallEndedReason;
}): void {
  const applied = useCalls.getState().applyEnded(params);
  if (!applied) return;
  if (params.reason === 'missed') useCalls.getState().markMissed(params.peer);
  const dict = dictionaries[useUi.getState().lang];
  const name = displayNameOf(useFriends.getState().contacts, params.peer);
  const toast = callEndedToast(dict, params.reason, name);
  if (toast !== null) useUi.getState().toast(toast.kind, toast.text);
  useVoice
    .getState()
    .sync()
    .catch(() => {
      // Best effort : l'état vocal se corrigera au prochain événement.
    });
}

/**
 * Notification click fallback: when the window regains focus shortly after a
 * native notification, navigate to the conversation that triggered it.
 * `ConversationRef` is structurally a `View`, so the UI store's `setView`
 * routes directly (home rail + DM, or server rail + channel).
 */
function useNotificationNavigation() {
  useEffect(() => {
    const onFocus = (): void => {
      const ref = takePendingConversation();
      if (ref !== null) useUi.getState().setView(ref);
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, []);
}

/**
 * Supprime le menu contextuel natif du système (« Recharger », hérité de la
 * vue web sous-jacente) partout sauf dans les champs de saisie, où le
 * copier/coller natif reste nécessaire. Les menus contextuels maison
 * (message, utilisateur, salon, serveur) font déjà leur propre
 * `preventDefault` avant d'ouvrir `ContextMenu` : `e.defaultPrevented` évite
 * alors tout travail redondant ici.
 */
function useSuppressNativeContextMenu(): void {
  useEffect(() => {
    const onContextMenu = (e: MouseEvent): void => {
      if (e.defaultPrevented || isEditableTarget(e.target)) return;
      e.preventDefault();
    };
    document.addEventListener('contextmenu', onContextMenu);
    return () => document.removeEventListener('contextmenu', onContextMenu);
  }, []);
}

/**
 * Raccourcis clavier globaux (voir Paramètres → Raccourcis clavier) :
 * - Ctrl/Cmd+K bascule le sélecteur rapide, y compris champ de saisie
 *   focalisé (convention Discord) ;
 * - Alt+↑/↓, Ctrl/Cmd+Maj+M sont ignorés tant qu'un champ éditable a le
 *   focus (`isEditableTarget`, même garde que le menu contextuel).
 * N'intercepte jamais Échap : menus et modales gèrent déjà leur fermeture
 * par leur propre écouteur (voir `QuickSwitcher`, `SettingsModal`, etc.).
 */
function useGlobalShortcuts(): void {
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent): void => {
      const mod = e.ctrlKey || e.metaKey;

      if (mod && !e.shiftKey && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        useUi.getState().toggleQuickSwitcher();
        return;
      }

      if (isEditableTarget(e.target)) return;

      if (mod && e.shiftKey && e.key.toLowerCase() === 'm') {
        e.preventDefault();
        useVoice
          .getState()
          .toggleMute()
          .catch(() => {
            // Best effort : l'état vocal se corrigera au prochain événement.
          });
        return;
      }

      if (e.altKey && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
        e.preventDefault();
        const direction = e.key === 'ArrowDown' ? 1 : -1;
        const view = useUi.getState().view;

        if (view.kind === 'group') {
          const state = useGroups.getState().states[view.groupId];
          if (state === undefined) return;
          const self = useSession.getState().self;
          const channels = visibleNavigableChannels(state, self?.pubkey ?? null);
          const nextChannelId = cycleChannel(channels, view.channelId, direction);
          if (nextChannelId !== null) {
            useUi.getState().setView({
              kind: 'group',
              groupId: view.groupId,
              channelId: nextChannelId,
            });
          }
          return;
        }

        const peers = useFriends
          .getState()
          .contacts.filter((c) => c.state === 'friend')
          .map((c) => c.pubkey);
        const currentPeer = view.kind === 'dm' ? view.peer : null;
        const nextPeer = cycleDm(peers, currentPeer, direction);
        if (nextPeer !== null) useUi.getState().setView({ kind: 'dm', peer: nextPeer });
      }
    };
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  }, []);
}

/** Câble les événements temps réel du nœud vers les stores. */
function useNodeEvents() {
  useEffect(() => {
    const off = rpc.onEvent((method, params) => {
      const event = { method, params } as AccordEvent;
      switch (event.method) {
        case 'event.dm': {
          const { peer, msg_id: msgId } = event.params;
          // Fusion incrémentale de la page récente (pas de rechargement
          // complet), puis notification éventuelle une fois l'auteur connu.
          void useDms
            .getState()
            .refresh(peer)
            .then(() => {
              const message = (useDms.getState().conversations[peer] ?? []).find(
                (m) => m.msg_id === msgId,
              );
              if (message !== undefined) {
                notifyNewMessage(
                  { kind: 'dm', peer },
                  message.author,
                  message.mentions_me === true,
                );
                maybePlaySound(
                  { kind: 'dm', peer },
                  message.author,
                  message.mentions_me === true,
                );
              }
            })
            .catch(() => {
              // Best effort : le fil sera rechargé au prochain événement.
            });
          // Conversation non affichée : rafraîchit le compteur de non-lus du
          // contact (la vue ouverte marque lue elle-même, ce qui recharge déjà).
          const view = useUi.getState().view;
          if (!(view.kind === 'dm' && view.peer === peer)) {
            void useFriends.getState().load();
          }
          break;
        }
        case 'event.dm_typing':
          useTyping
            .getState()
            .noteTyping(dmTypingKey(event.params.peer), event.params.peer);
          break;
        case 'event.friend_request':
        case 'event.friend_response':
          void useFriends.getState().load();
          break;
        case 'event.profile':
          // Profil d'un ami annoncé : pseudo, bio et avatar mis à jour en
          // place, sans repasser par le nœud.
          useFriends.getState().applyProfile(event.params);
          break;
        case 'event.presence':
          // Présence best-effort d'un ami : reflétée sur le contact connu.
          useFriends.getState().applyPresence(event.params.pubkey, event.params.online);
          break;
        case 'event.group_op':
        case 'event.group_key':
          void useGroups.getState().loadList();
          break;
        case 'event.group_invite_pending':
          useGroups.getState().handleInvitePending(event.params);
          maybePlayInviteSound();
          break;
        case 'event.group_state':
          // Op appliquée (locale ou distante) : recharger l'état du groupe.
          void useGroups
            .getState()
            .handleGroupState(event.params.group_id)
            .catch(() => {
              // Best effort : l'état sera rechargé au prochain événement.
            });
          break;
        case 'event.group_msg': {
          const {
            group_id: groupId,
            channel_id: channelId,
            msg_id: msgId,
          } = event.params;
          void useGroups
            .getState()
            .refreshHistory(groupId, channelId)
            .then(() => {
              const key = channelKey(groupId, channelId);
              const message = (useGroups.getState().messages[key] ?? []).find(
                (m) => m.msg_id === msgId,
              );
              if (message !== undefined) {
                notifyNewMessage(
                  { kind: 'group', groupId, channelId },
                  message.author,
                  message.mentions_me === true,
                );
                maybePlaySound(
                  { kind: 'group', groupId, channelId },
                  message.author,
                  message.mentions_me === true,
                );
              }
            })
            .catch(() => {
              // Best effort : l'historique sera rechargé au prochain événement.
            });
          // Salon non affiché : rafraîchit les compteurs de non-lus (le salon
          // ouvert marque lu lui-même, ce qui rafraîchit déjà).
          const view = useUi.getState().view;
          const displayed =
            view.kind === 'group' &&
            view.groupId === groupId &&
            view.channelId === channelId;
          if (!displayed) {
            void useGroups
              .getState()
              .refreshUnread()
              .catch(() => {
                // Best effort : compteurs corrigés au prochain passage.
              });
          }
          break;
        }
        case 'event.group_typing': {
          const { group_id: groupId, channel_id: channelId, pubkey } = event.params;
          useTyping.getState().noteTyping(groupTypingKey(groupId, channelId), pubkey);
          break;
        }
        case 'event.voice_joined':
          useVoice.getState().applyJoined(event.params);
          break;
        case 'event.voice_left':
          useVoice.getState().applyLeft(event.params);
          break;
        case 'event.voice_speaking':
          useVoice.getState().applySpeaking(event.params);
          break;
        case 'event.voice_mute':
          useVoice.getState().applyMuteState(event.params);
          break;
        case 'event.voice_moderate':
          useVoice.getState().applyVoiceModerate(event.params);
          break;
        case 'event.call_outgoing':
          useCalls.getState().applyOutgoing(event.params);
          break;
        case 'event.call_incoming':
          useCalls.getState().applyIncoming(event.params);
          break;
        case 'event.call_accepted':
          useCalls.getState().applyAccepted(event.params);
          // La session d'appel réutilise le moteur vocal existant (sentinelle
          // group_id) : on la fait apparaître via le flux normal voice.status,
          // plutôt que de dupliquer sa forme dans le store d'appel.
          useVoice
            .getState()
            .sync()
            .catch(() => {
              // Best effort : l'état vocal se corrigera au prochain événement.
            });
          break;
        case 'event.call_ended':
          handleCallEnded(event.params);
          break;
        case 'event.desynchronise': {
          void useFriends.getState().load();
          void useGroups.getState().loadList();
          break;
        }
        case 'event.group_event_started':
          notifyEventStarted(event.params.title);
          break;
      }
    });
    return off;
  }, []);
}

/**
 * Statut de présence forcé au démarrage (Paramètres → Confidentialité →
 * « Statut au démarrage »), une seule fois par session : dès que la session
 * passe à `ready`, si une préférence est choisie (`null` = ne rien forcer),
 * l'applique via l'action existante `friends.setOwnStatus`. Le nœud a déjà
 * son propre statut persisté (chargé par `UserPanel` via `loadOwnStatus`) —
 * ce réglage ne fait que le remplacer une fois, pas à chaque reconnexion.
 */
function useStartupPresence(): void {
  const phase = useSession((s) => s.phase);
  const startupPresence = useUi((s) => s.startupPresence);
  const appliedRef = useRef(false);

  useEffect(() => {
    if (phase !== 'ready' || appliedRef.current) return;
    appliedRef.current = true;
    if (startupPresence === null) return;
    useFriends
      .getState()
      .setOwnStatus(startupPresence)
      .catch(() => {
        // Best effort : la présence locale garde le statut persisté par le nœud.
      });
  }, [phase, startupPresence]);
}

/**
 * Pastille du dock / de la barre des tâches : reflète le total de messages
 * privés non lus et de mentions de serveur. Recalculée à chaque changement des
 * compteurs (contacts, mentions de groupe) et poussée à l'OS via l'hôte Tauri.
 */
function useUnreadBadge(): void {
  const contacts = useFriends((s) => s.contacts);
  const mentions = useGroups((s) => s.mentions);
  useEffect(() => {
    setAppBadge(unreadBadgeTotal(totalDmUnread(contacts), mentions));
  }, [contacts, mentions]);
}

export function AppShell() {
  const t = useT();
  const view = useUi((s) => s.view);
  const theme = useUi((s) => s.theme);
  const toast = useUi((s) => s.toast);
  const sidebarWidth = useUi((s) => s.sidebarWidth);
  const setSidebarWidth = useUi((s) => s.setSidebarWidth);
  const loadFriends = useFriends((s) => s.load);
  const loadGroups = useGroups((s) => s.loadList);
  const syncVoice = useVoice((s) => s.sync);
  const syncCalls = useCalls((s) => s.sync);
  useNodeEvents();
  useNotificationNavigation();
  useSuppressNativeContextMenu();
  useStartupPresence();
  useUnreadBadge();
  useGlobalShortcuts();

  // Appui-pour-parler global : actif dès qu'un salon vocal est rejoint.
  const onPttError = useCallback(() => toast('error', t.errors.actionFailed), [toast, t]);
  usePushToTalk(onPttError);

  useEffect(() => {
    loadFriends().catch(() => {
      // Best effort : sans réponse du nœud, la liste d'amis reste vide.
    });
    loadGroups().catch(() => {
      // Best effort : sans réponse du nœud, la liste de serveurs reste vide.
    });
    // Reprise vocale : resynchronise le salon actif éventuel (voice.status).
    syncVoice().catch(() => {
      // Best effort : sans réponse du nœud, l'état vocal local reste vide.
    });
    // Reprise d'appel : resynchronise la phase courante (calls.status).
    syncCalls().catch(() => {
      // Best effort : sans réponse du nœud, l'état d'appel local reste idle.
    });
  }, [loadFriends, loadGroups, syncVoice, syncCalls]);

  return (
    <div className="app-ambient accord-shell flex h-full">
      {/*
       * Lien d'évitement : premier élément tabbable, visible uniquement au
       * focus clavier — saute rail et barre latérale pour atteindre la zone
       * de contenu (`main`, rendu focusable par tabIndex={-1}).
       */}
      <a
        href="#contenu-principal"
        className="sr-only focus:not-sr-only focus:absolute focus:left-2 focus:top-2 focus:z-[70] focus:rounded-md focus:bg-blurple focus:px-3 focus:py-2 focus:text-sm focus:font-medium focus:text-white"
      >
        {t.layout.skipToContent}
      </a>
      <ServerRail />
      <Sidebar />
      <ResizeHandle
        value={sidebarWidth}
        min={SIDEBAR_WIDTH_MIN}
        max={SIDEBAR_WIDTH_MAX}
        defaultValue={SIDEBAR_WIDTH_DEFAULT}
        onChange={setSidebarWidth}
        ariaLabel={t.layout.resizeSidebar}
        panelSide="left"
        ringOffsetClassName="ring-offset-sidebar"
      />
      <main
        id="contenu-principal"
        tabIndex={-1}
        className="theme-surface-chat accord-stage min-w-0 flex-1 bg-chat focus:outline-none"
        aria-label={t.app.name}
      >
        <ThemeAtmosphere theme={theme} />
        {/*
         * Garde-fou : un crash de la vue courante affiche un repli local ;
         * rail, barre latérale et navigation restent utilisables, et changer
         * de vue (resetKey) retente un rendu normal.
         */}
        <ErrorBoundary
          resetKey={
            view.kind === 'dm'
              ? `dm:${view.peer}`
              : view.kind === 'group'
                ? `group:${view.groupId}:${view.channelId ?? ''}`
                : view.kind
          }
        >
          {view.kind === 'friends' && <FriendsView />}
          {view.kind === 'dm' && <DmView peer={view.peer} />}
          {view.kind === 'group' && (
            <GroupView groupId={view.groupId} channelId={view.channelId} />
          )}
        </ErrorBoundary>
      </main>
      <Modals />
      <ProfilePopover />
      <ContextMenu />
      <IncomingCall />
      <QuickSwitcher />
    </div>
  );
}
