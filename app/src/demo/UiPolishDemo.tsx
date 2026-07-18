import React from 'react';
import ReactDOM from 'react-dom/client';
import type {
  Contact,
  DmMessage,
  GroupMessage,
  GroupStateJson,
  SelfProfile,
} from '../lib/api';
import { AppShell } from '../components/AppShell';
import { Toasts } from '../components/Toasts';
import { THEME_LABEL_KEYS } from '../components/settings/AppearanceTab';
import { useCalls } from '../stores/calls';
import { useDms } from '../stores/dms';
import { useFriends } from '../stores/friends';
import { channelKey, useGroups } from '../stores/groups';
import { useSession } from '../stores/session';
import { groupTypingKey, useTyping } from '../stores/typing';
import { THEME_IDS, useT, useUi, type Theme, type View } from '../stores/ui';
import { useVoice } from '../stores/voice';
import '../styles/global.css';
import '../styles/theme-scenes.css';
import '../styles/profile-personalization.css';
import '../styles/profile-personalization-extra.css';
import '../styles/profile-personalization-more.css';
import '../styles/profile-surfaces.css';
import '../styles/identity-refresh.css';
import '../styles/liquid-glass.css';

const SELF_ID = 'demo-self';
const GROUP_ID = 'demo-cipher';
const CHANNEL_ID = 'general';
const now = Date.now();

const SELF: SelfProfile = {
  node_id: 'node-demo-self',
  pubkey: SELF_ID,
  friend_code: 'accord-ambre-orbite-2048',
  name: 'Ari Vale',
  bio: 'Design systems, cryptographie et café très serré.',
  avatar: null,
  banner: null,
  pronouns: 'iel / elle',
  accent_color: 0x7c6df2,
  banner_color: 0x27244a,
  avatar_decoration: 'golden_laurel',
  profile_effect: 'aurora',
  profile_frame: 'lumen_bloom',
};

const CONTACTS: Contact[] = [
  {
    node_id: 'node-noa',
    pubkey: 'noa',
    friend_code: 'accord-noa-cobalt-1701',
    display_name: 'Noa Chen',
    bio: 'Architecture distribuée et observabilité.',
    avatar: null,
    banner: null,
    pronouns: 'il / lui',
    accent_color: 0x42b7a3,
    banner_color: 0x173c3a,
    avatar_decoration: 'aurora_ring',
    profile_effect: 'starfield',
    profile_frame: 'crystal_crown',
    state: 'friend',
    last_seen_ms: now,
    online: true,
    status: 'online',
    status_text: 'En train de relire le protocole',
    unread: 3,
    mention_count: 1,
    read_lamport: 2,
  },
  {
    node_id: 'node-mina',
    pubkey: 'mina',
    friend_code: 'accord-mina-sakura-9920',
    display_name: 'Mina Sol',
    bio: 'Illustration, motion et interfaces calmes.',
    avatar: null,
    banner: null,
    pronouns: 'elle',
    accent_color: 0xe86f9b,
    banner_color: 0x4a2437,
    avatar_decoration: 'sakura_arc',
    profile_effect: 'falling_petals',
    profile_frame: 'celestial_wings',
    state: 'friend',
    last_seen_ms: now - 86_000,
    online: true,
    status: 'idle',
    status_text: 'Pause thé',
    unread: 0,
  },
  {
    node_id: 'node-ezra',
    pubkey: 'ezra',
    friend_code: 'accord-ezra-pixel-4412',
    display_name: 'Ezra Ko',
    bio: 'Builds minuscules, idées immenses.',
    avatar: null,
    banner: null,
    avatar_decoration: 'pixel_crown',
    profile_effect: 'floating_particles',
    profile_frame: 'neon_circuit',
    state: 'friend',
    last_seen_ms: now - 3_600_000,
    online: false,
    status: 'dnd',
    status_text: 'Concentration',
    unread: 1,
  },
  {
    node_id: 'node-liv',
    pubkey: 'liv',
    friend_code: 'accord-liv-prisme-3308',
    display_name: 'Liv Morgan',
    bio: null,
    avatar: null,
    banner: null,
    state: 'pending_in',
    last_seen_ms: now - 8_000,
  },
];

const GROUP_STATE: GroupStateJson = {
  group_id: GROUP_ID,
  name: 'Atelier Cipher',
  icon: null,
  banner: null,
  banner_color: 0x35306b,
  founder: SELF_ID,
  members: [
    { pubkey: SELF_ID, roles: ['lead'], nickname: 'Ari', timeout_until_ms: 0 },
    { pubkey: 'noa', roles: ['core'], nickname: null, timeout_until_ms: 0 },
    { pubkey: 'mina', roles: ['design'], nickname: 'Mina ✦', timeout_until_ms: 0 },
    { pubkey: 'ezra', roles: [], nickname: null, timeout_until_ms: 0 },
  ],
  bans: [],
  channels: [
    {
      channel_id: CHANNEL_ID,
      name: 'général',
      kind: 'text',
      category: 'studio',
      position: 0,
      topic: 'Décisions produit, idées et petites victoires du jour.',
    },
    {
      channel_id: 'design',
      name: 'design-lab',
      kind: 'text',
      category: 'studio',
      position: 1,
      topic: 'Explorations visuelles et retours.',
    },
    {
      channel_id: 'annonces',
      name: 'annonces',
      kind: 'announcement',
      category: 'studio',
      position: 2,
      topic: 'Les informations importantes de l’atelier.',
    },
    {
      channel_id: 'vocal',
      name: 'Table ronde',
      kind: 'voice',
      category: 'live',
      position: 3,
      topic: '',
    },
    {
      channel_id: 'forum',
      name: 'propositions',
      kind: 'forum',
      category: 'live',
      position: 4,
      topic: 'Une idée par fil.',
    },
  ],
  categories: [
    { category_id: 'studio', name: 'Studio', position: 0 },
    { category_id: 'live', name: 'En direct', position: 1 },
  ],
  roles: [
    {
      role_id: 'lead',
      name: 'Direction',
      color: 0xf3b95f,
      position: 3,
      permissions: 0x1ff,
    },
    { role_id: 'core', name: 'Core', color: 0x54c7b0, position: 2, permissions: 0x3 },
    { role_id: 'design', name: 'Design', color: 0xe77aa8, position: 1, permissions: 0x3 },
  ],
  invites: [],
  emojis: [],
  stickers: [],
  sounds: [],
  events: [
    {
      event_id: 'review',
      title: 'Revue hebdomadaire',
      description: 'Décisions et démonstrations.',
      start_ms: now + 7_200_000,
      channel_id: 'vocal',
      author: SELF_ID,
      rsvp_count: 3,
      rsvped: true,
    },
  ],
  threads: [
    {
      thread_id: 'thread-layout',
      parent_channel: CHANNEL_ID,
      root_msg: 'group-3',
      name: 'Rendre les panneaux plus souples',
      archived: false,
    },
  ],
  polls: [],
  automod_words: [],
  read_marks: { [CHANNEL_ID]: 2 },
  my_permissions: 0x1ff,
};

const GROUP_MESSAGES: GroupMessage[] = [
  {
    msg_id: 'group-1',
    channel_id: CHANNEL_ID,
    author: 'noa',
    lamport: 1,
    sent_ms: now - 45 * 60_000,
    deleted: false,
    body: {
      type: 'text',
      text: 'La synchronisation reprend proprement après une coupure. Le nouvel indicateur est beaucoup plus lisible.',
      reply_to: null,
      attachments: 0,
    },
    edited: null,
    reactions: [
      { emoji: '✨', author: SELF_ID },
      { emoji: '✨', author: 'mina' },
    ],
    attachments: [],
  },
  {
    msg_id: 'group-2',
    channel_id: CHANNEL_ID,
    author: SELF_ID,
    lamport: 2,
    sent_ms: now - 37 * 60_000,
    deleted: false,
    body: {
      type: 'text',
      text: 'Parfait. Je garde le contraste calme, mais les états actifs doivent être impossibles à rater.',
      reply_to: 'group-1',
      attachments: 0,
    },
    edited: null,
    reactions: [{ emoji: '👍', author: 'noa' }],
    attachments: [],
  },
  {
    msg_id: 'group-3',
    channel_id: CHANNEL_ID,
    author: 'mina',
    lamport: 3,
    sent_ms: now - 12 * 60_000,
    deleted: false,
    mentions_me: true,
    body: {
      type: 'text',
      text: '@Ari j’ai ouvert un fil pour la composition responsive. Le panneau membres ne doit plus écraser la conversation.',
      reply_to: null,
      attachments: 0,
    },
    edited: null,
    reactions: [{ emoji: '💡', author: 'ezra' }],
    attachments: [],
  },
  {
    msg_id: 'group-4',
    channel_id: CHANNEL_ID,
    author: 'ezra',
    lamport: 4,
    sent_ms: now - 3 * 60_000,
    deleted: false,
    body: {
      type: 'text',
      text: 'Je teste la fenêtre minimale et les textes longs. Pour l’instant tout reste stable — même avec les panneaux ouverts.',
      reply_to: null,
      attachments: 0,
    },
    edited: null,
    reactions: [],
    attachments: [],
  },
];

const DM_MESSAGES: DmMessage[] = [
  {
    msg_id: 'dm-1',
    author: 'noa',
    lamport: 1,
    sent_ms: now - 16 * 60_000,
    acked: true,
    delivery: 'sent',
    deleted: false,
    body: {
      type: 'text',
      text: 'Tu peux regarder le dernier prototype quand tu as une minute ?',
      reply_to: null,
      attachments: 0,
    },
    edited: null,
    reactions: [],
    attachments: [],
  },
  {
    msg_id: 'dm-2',
    author: SELF_ID,
    lamport: 2,
    sent_ms: now - 9 * 60_000,
    acked: true,
    delivery: 'sent',
    deleted: false,
    body: {
      type: 'text',
      text: 'Oui — la hiérarchie est nette. Je peaufine juste le rythme et les petits états.',
      reply_to: 'dm-1',
      attachments: 0,
    },
    edited: null,
    reactions: [{ emoji: '👌', author: 'noa' }],
    attachments: [],
  },
];

const noop = async (): Promise<void> => {};

function seedDemoStores(): void {
  useSession.setState({ phase: 'ready', self: SELF, askName: false, error: null });
  useFriends.setState({
    contacts: CONTACTS,
    loaded: true,
    ownStatus: 'online',
    ownStatusText: 'Polissage de l’interface',
    load: noop,
    loadOwnStatus: noop,
    markRead: noop,
  });
  useGroups.setState({
    ids: [GROUP_ID],
    states: { [GROUP_ID]: GROUP_STATE },
    messages: { [channelKey(GROUP_ID, CHANNEL_ID)]: GROUP_MESSAGES },
    hasMore: { [channelKey(GROUP_ID, CHANNEL_ID)]: false },
    loadingOlder: {},
    pins: { [channelKey(GROUP_ID, CHANNEL_ID)]: ['group-1'] },
    unread: { [GROUP_ID]: { design: 4 } },
    mentions: { [GROUP_ID]: 1 },
    channelMentions: { [GROUP_ID]: { [CHANNEL_ID]: 1 } },
    pendingInvites: [],
    loadList: noop,
    refreshHistory: noop,
    loadPins: noop,
    markRead: noop,
  });
  useDms.setState({
    conversations: { noa: DM_MESSAGES },
    hasMore: { noa: false },
    loadingOlder: {},
    pins: { noa: ['dm-1'] },
    peerRead: { noa: 1 },
    refresh: noop,
    loadPins: noop,
  });
  useVoice.setState({
    active: null,
    participants: new Map(),
    rooms: new Map(),
    sync: noop,
  });
  useCalls.setState({
    phase: 'idle',
    peer: null,
    callId: null,
    sincePhaseMs: null,
    missedPeers: new Set(['ezra']),
    sync: noop,
  });
  useTyping.setState({
    writers: {
      [groupTypingKey(GROUP_ID, CHANNEL_ID)]: { mina: Number.MAX_SAFE_INTEGER },
    },
  });
  useUi.setState({
    lang: 'fr',
    view: { kind: 'group', groupId: GROUP_ID, channelId: CHANNEL_ID },
    modal: null,
    profile: null,
    quickSwitcherOpen: false,
    sidebarWidth: 240,
    membersWidth: 240,
    theme: 'dark',
    density: 'comfortable',
  });
  useUi.getState().setTheme('dark');
}

seedDemoStores();

function DemoToolbar() {
  const t = useT();
  const theme = useUi((state) => state.theme);
  const view = useUi((state) => state.view);
  const setTheme = useUi((state) => state.setTheme);
  const setView = useUi((state) => state.setView);
  const openModal = useUi((state) => state.openModal);

  const views: Array<{ label: string; view: View }> = [
    { label: 'Salon', view: { kind: 'group', groupId: GROUP_ID, channelId: CHANNEL_ID } },
    { label: 'MP', view: { kind: 'dm', peer: 'noa' } },
    { label: 'Amis', view: { kind: 'friends' } },
  ];
  const themes: Array<{ label: string; theme: Theme }> = THEME_IDS.map((id) => ({
    label: t.settings[THEME_LABEL_KEYS[id]],
    theme: id,
  }));

  return (
    <div className="flex h-11 shrink-0 items-center gap-3 border-b border-[color:var(--glass-border)] bg-rail px-4 shadow-1">
      <span className="text-xs font-semibold uppercase tracking-[0.16em] text-muted">
        Aperçu UI
      </span>
      <span className="h-5 w-px bg-input" aria-hidden />
      <div className="flex items-center gap-1" aria-label="Vues de démonstration">
        {views.map((item) => {
          const active =
            item.view.kind === view.kind &&
            (item.view.kind !== 'group' ||
              (view.kind === 'group' && item.view.groupId === view.groupId));
          return (
            <button
              key={item.label}
              type="button"
              aria-pressed={active}
              onClick={() => setView(item.view)}
              className={`min-h-8 rounded-full px-3 text-xs font-medium transition-colors duration-fast ${
                active
                  ? 'bg-blurple text-white'
                  : 'text-muted hover:bg-sidebar hover:text-header'
              }`}
            >
              {item.label}
            </button>
          );
        })}
      </div>
      <button
        type="button"
        onClick={() => openModal({ kind: 'settings' })}
        className="min-h-8 rounded-full px-3 text-xs font-medium text-muted transition-colors duration-fast hover:bg-sidebar hover:text-header"
      >
        Réglages
      </button>
      <label className="ml-auto flex items-center gap-2 text-xs font-medium text-muted">
        Thème
        <select
          aria-label="Thème de démonstration"
          value={theme}
          onChange={(event) => setTheme(event.target.value as Theme)}
          className="min-h-8 max-w-44 rounded-md border border-input bg-sidebar px-2 text-xs font-medium text-header transition-colors duration-fast focus:border-blurple/50 focus-visible:ring-2 focus-visible:ring-header"
        >
          {themes.map((item) => (
            <option key={item.theme} value={item.theme}>
              {item.label}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

function UiPolishDemo() {
  return (
    <div className="flex h-full flex-col bg-chat">
      <DemoToolbar />
      <div className="min-h-0 flex-1">
        <AppShell />
      </div>
      <Toasts />
    </div>
  );
}

const root = document.getElementById('root');
if (root === null) throw new Error('élément racine introuvable');

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <UiPolishDemo />
  </React.StrictMode>,
);
