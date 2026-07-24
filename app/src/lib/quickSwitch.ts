/**
 * Pure quick-switcher and navigation helpers. This module deliberately has no
 * React or DOM dependencies so its result building and ranking stay testable.
 */

import type { Contact, GroupChannel, GroupChannelKind, GroupStateJson } from './api';
import { channelsByCategory, isChannelVisible } from '../stores/groups';
import type { View } from '../stores/ui';

/** Stable ID for a direct-message result. */
export function dmItemId(pubkey: string): string {
  return `dm:${pubkey}`;
}

/** Stable ID for a server-channel result. */
export function channelItemId(groupId: string, channelId: string): string {
  return `channel:${groupId}/${channelId}`;
}

/** Stable ID for a server result. */
export function serverItemId(groupId: string): string {
  return `server:${groupId}`;
}

interface QuickSwitchItemBase {
  /** Stable key, unique across the result list. */
  id: string;
  /** Primary display label and highest-priority search field. */
  label: string;
  view: View;
}

/** Special Friends/Home destination. */
export interface FriendsSwitchItem extends QuickSwitchItemBase {
  kind: 'friends';
}

/** Direct conversation with an established friend. */
export interface DmSwitchItem extends QuickSwitchItemBase {
  kind: 'dm';
  pubkey: string;
  avatarHash: string | null;
  avatarDecoration: string | null;
}

/** Locally visible text, announcement, or voice channel. */
export interface ChannelSwitchItem extends QuickSwitchItemBase {
  kind: 'channel';
  /** Server name shown as secondary text and included in search. */
  subtitle: string;
  channelKind: GroupChannelKind;
  groupId: string;
  channelId: string;
}

/** Joined server whose destination is resolved when selected. */
export interface ServerSwitchItem {
  id: string;
  kind: 'server';
  label: string;
  groupId: string;
}

export type QuickSwitchCommandIcon =
  | 'appearance'
  | 'calendar'
  | 'category'
  | 'channel'
  | 'copy'
  | 'deafen'
  | 'dm'
  | 'headphones'
  | 'invite'
  | 'leave'
  | 'mark-read'
  | 'microphone'
  | 'muted-channels'
  | 'notifications'
  | 'privacy'
  | 'server'
  | 'settings'
  | 'status'
  | 'theme'
  | 'unread'
  | 'voice';

/** Store-backed action exposed by the command palette. */
export interface CommandSwitchItem {
  id: string;
  kind: 'command';
  label: string;
  subtitle: string;
  keywords?: readonly string[];
  shortcut?: string;
  danger?: boolean;
  icon?: QuickSwitchCommandIcon;
  featured?: boolean;
  customStatusMode?: 'edit' | 'clear';
  run: () => void;
}

export type QuickSwitchItem =
  | FriendsSwitchItem
  | DmSwitchItem
  | ChannelSwitchItem
  | ServerSwitchItem
  | CommandSwitchItem;

export type QuickSwitchSectionId = 'recent' | 'channels' | 'dms' | 'servers' | 'commands';

export interface QuickSwitchSection {
  id: QuickSwitchSectionId;
  items: QuickSwitchItem[];
}

export const QUICK_SWITCH_SECTION_ORDER = [
  'channels',
  'dms',
  'servers',
  'commands',
] as const satisfies readonly Exclude<QuickSwitchSectionId, 'recent'>[];

export function quickSwitchSectionId(
  item: QuickSwitchItem,
): Exclude<QuickSwitchSectionId, 'recent'> {
  switch (item.kind) {
    case 'channel':
      return 'channels';
    case 'friends':
    case 'dm':
      return 'dms';
    case 'server':
      return 'servers';
    case 'command':
      return 'commands';
  }
}

/** Groups ranked items without changing their order within a section. */
export function sectionQuickSwitchItems(
  items: readonly QuickSwitchItem[],
  recent = false,
): QuickSwitchSection[] {
  if (items.length === 0) return [];
  if (recent) return [{ id: 'recent', items: [...items] }];

  const grouped = new Map<Exclude<QuickSwitchSectionId, 'recent'>, QuickSwitchItem[]>();
  for (const item of items) {
    const id = quickSwitchSectionId(item);
    const section = grouped.get(id);
    if (section === undefined) grouped.set(id, [item]);
    else section.push(item);
  }
  return QUICK_SWITCH_SECTION_ORDER.flatMap((id) => {
    const section = grouped.get(id);
    return section === undefined ? [] : [{ id, items: section }];
  });
}

/** Builds all locally visible navigation destinations. */
export function buildQuickSwitchItems(params: {
  friendsLabel: string;
  contacts: readonly Contact[];
  groupIds: readonly string[];
  groupStates: Readonly<Record<string, GroupStateJson>>;
  selfPubkey: string | null;
}): QuickSwitchItem[] {
  const items: QuickSwitchItem[] = [
    {
      id: 'friends',
      kind: 'friends',
      label: params.friendsLabel,
      view: { kind: 'friends' },
    },
  ];

  for (const contact of params.contacts) {
    if (contact.state !== 'friend') continue;
    items.push({
      id: dmItemId(contact.pubkey),
      kind: 'dm',
      label:
        contact.display_name.trim() !== '' ? contact.display_name : contact.friend_code,
      pubkey: contact.pubkey,
      avatarHash: contact.avatar,
      avatarDecoration: contact.avatar_decoration ?? null,
      view: { kind: 'dm', peer: contact.pubkey },
    });
  }

  for (const groupId of params.groupIds) {
    const state = params.groupStates[groupId];
    if (state === undefined) continue;
    items.push({ id: serverItemId(groupId), kind: 'server', label: state.name, groupId });
    for (const channel of state.channels) {
      if (!isChannelVisible(state, channel.channel_id, params.selfPubkey)) continue;
      items.push({
        id: channelItemId(groupId, channel.channel_id),
        kind: 'channel',
        label: channel.name,
        subtitle: state.name,
        channelKind: channel.kind,
        groupId,
        channelId: channel.channel_id,
        view: { kind: 'group', groupId, channelId: channel.channel_id },
      });
    }
  }

  return items;
}

/** Builds recent destinations from the existing navigation memory. */
export function buildRecentItems(
  items: readonly QuickSwitchItem[],
  groupIds: readonly string[],
  lastChannelByServer: Readonly<Record<string, string>>,
  lastDmPeer: string | null,
): QuickSwitchItem[] {
  const byId = new Map(items.map((item) => [item.id, item] as const));
  const recent: QuickSwitchItem[] = [];

  if (lastDmPeer !== null) {
    const dm = byId.get(dmItemId(lastDmPeer));
    if (dm !== undefined) recent.push(dm);
  }
  for (const groupId of groupIds) {
    const channelId = lastChannelByServer[groupId];
    if (channelId === undefined) continue;
    const channel = byId.get(channelItemId(groupId, channelId));
    if (channel !== undefined) recent.push(channel);
  }
  return recent;
}

/* ------------------------------------------------------------------ */
/* Fuzzy ranking.                                                      */
/* ------------------------------------------------------------------ */

const SCORE_SUBSEQUENCE = 1;
const SCORE_SUBSTRING = 2;
const SCORE_WORD_BOUNDARY = 3;
const SCORE_PREFIX = 4;
const SCORE_FIELD_SCALE = 10;
const SCORE_LABEL_BONUS = 2;
const SCORE_SUBTITLE_BONUS = 1;

function normalizeSearchText(text: string): string {
  return text
    .normalize('NFKD')
    .replace(/\p{M}+/gu, '')
    .toLowerCase();
}

/** Splits text on non-alphanumeric boundaries. */
function splitWords(text: string): string[] {
  return text.split(/[^\p{L}\p{N}]+/u).filter((word) => word !== '');
}

/** Whether query characters appear in text in order. */
function isSubsequence(query: string, text: string): boolean {
  let i = 0;
  for (const ch of text) {
    if (i >= query.length) break;
    if (ch === query[i]) i += 1;
  }
  return i === query.length;
}

/** Prefix > word prefix > substring > ordered subsequence. */
export function matchScore(label: string, query: string): number | null {
  const q = normalizeSearchText(query.trim());
  if (q === '') return null;
  const text = normalizeSearchText(label);
  if (text.startsWith(q)) return SCORE_PREFIX;
  if (splitWords(text).some((word) => word.startsWith(q))) return SCORE_WORD_BOUNDARY;
  if (text.includes(q)) return SCORE_SUBSTRING;
  if (isSubsequence(q, text)) return SCORE_SUBSEQUENCE;
  return null;
}

export interface QuickSwitchSearchable {
  label: string;
  subtitle?: string;
  keywords?: readonly string[];
}

function weightedScore(score: number | null, bonus: number): number | null {
  return score === null ? null : score * SCORE_FIELD_SCALE + bonus;
}

/** Ranks by the strongest searchable field, with a small preference for labels. */
export function rankQuickSwitchItems<T extends QuickSwitchSearchable>(
  items: readonly T[],
  query: string,
): T[] {
  const scored: Array<{ item: T; score: number; index: number }> = [];
  for (const [index, item] of items.entries()) {
    let score = weightedScore(matchScore(item.label, query), SCORE_LABEL_BONUS);
    if (item.subtitle !== undefined) {
      const subtitleScore = weightedScore(
        matchScore(item.subtitle, query),
        SCORE_SUBTITLE_BONUS,
      );
      if (subtitleScore !== null && (score === null || subtitleScore > score)) {
        score = subtitleScore;
      }
    }
    for (const keyword of item.keywords ?? []) {
      const keywordScore = weightedScore(matchScore(keyword, query), 0);
      if (keywordScore !== null && (score === null || keywordScore > score)) {
        score = keywordScore;
      }
    }
    if (score !== null) scored.push({ item, score, index });
  }
  scored.sort(
    (a, b) =>
      b.score - a.score || a.item.label.localeCompare(b.item.label) || a.index - b.index,
  );
  return scored.map((s) => s.item);
}

/**
 * Finds the next unread item in display order, wrapping once. If the current
 * item is the only unread one it is returned after the wrap.
 */
export function nextUnreadItem<T extends { id: string }>(
  items: readonly T[],
  currentId: string | null,
  isUnread: (item: T) => boolean,
): T | null {
  if (items.length === 0) return null;
  const currentIndex =
    currentId === null ? -1 : items.findIndex((item) => item.id === currentId);
  const startIndex = currentIndex === -1 ? 0 : (currentIndex + 1) % items.length;
  for (let offset = 0; offset < items.length; offset += 1) {
    const item = items[(startIndex + offset) % items.length];
    if (item !== undefined && isUnread(item)) return item;
  }
  return null;
}

/** Returns visible server channels in sidebar order. */
export function visibleNavigableChannels(
  state: Pick<
    GroupStateJson,
    'channels' | 'categories' | 'my_permissions' | 'members' | 'overrides'
  >,
  selfPubkey: string | null,
): GroupChannel[] {
  const visible = state.channels.filter((c) =>
    isChannelVisible(state, c.channel_id, selfPubkey),
  );
  return channelsByCategory(visible, state.categories).flatMap(
    (section) => section.channels,
  );
}

/** Cycles through non-voice channels, wrapping at either end. */
export function cycleChannel(
  channels: readonly GroupChannel[],
  currentChannelId: string | null,
  direction: 1 | -1,
): string | null {
  const navigable = channels.filter((c) => c.kind !== 'voice');
  if (navigable.length === 0) return null;
  const index = navigable.findIndex((c) => c.channel_id === currentChannelId);
  if (index === -1) {
    return direction === 1
      ? navigable[0]!.channel_id
      : navigable[navigable.length - 1]!.channel_id;
  }
  const nextIndex = (index + direction + navigable.length) % navigable.length;
  return navigable[nextIndex]!.channel_id;
}

/** Cycles through direct-message peers with the same wrapping semantics. */
export function cycleDm(
  peers: readonly string[],
  currentPeer: string | null,
  direction: 1 | -1,
): string | null {
  if (peers.length === 0) return null;
  const index = peers.findIndex((p) => p === currentPeer);
  if (index === -1) return direction === 1 ? peers[0]! : peers[peers.length - 1]!;
  const nextIndex = (index + direction + peers.length) % peers.length;
  return peers[nextIndex]!;
}

/** Whether the platform is macOS. */
export function isMacPlatform(platform: string = navigator.platform): boolean {
  return /mac/i.test(platform);
}
