/**
 * Catalogue des onglets de paramètres : ajouter un onglet se résume à une
 * entrée dans `SETTINGS_GROUPS` (libellé i18n + composant de contenu).
 */

import type { ComponentType } from 'react';
import type { Dict, SettingsDict } from '../../i18n';
import { AccessibilityTab } from './AccessibilityTab';
import { AccountTab } from './AccountTab';
import { AdvancedTab } from './AdvancedTab';
import { AppearanceTab } from './AppearanceTab';
import { LanguageTab } from './LanguageTab';
import { NotificationsTab } from './NotificationsTab';
import { PlanningTab } from './PlanningTab';
import { PrivacyTab } from './PrivacyTab';
import { SecurityTab } from './SecurityTab';
import { ShortcutsTab } from './ShortcutsTab';
import { SystemTab } from './SystemTab';
import { TextMediaTab } from './TextMediaTab';
import { UpdatesTab } from './UpdatesTab';
import { VoiceTab } from './VoiceTab';

export type SettingsTabId =
  | 'account'
  | 'privacy'
  | 'security'
  | 'appearance'
  | 'accessibility'
  | 'textMedia'
  | 'language'
  | 'shortcuts'
  | 'voice'
  | 'notifications'
  | 'planning'
  | 'system'
  | 'updates'
  | 'advanced';

export interface SettingsTab {
  id: SettingsTabId;
  label: (t: Dict, ts: SettingsDict) => string;
  Content: ComponentType;
}

export interface SettingsGroup {
  id: string;
  label: (t: Dict, ts: SettingsDict) => string;
  tabs: SettingsTab[];
}

/** Onglet ouvert par défaut. */
export const DEFAULT_TAB: SettingsTab = {
  id: 'account',
  label: (_t, ts) => ts.settings.account,
  Content: AccountTab,
};

export const SETTINGS_GROUPS: SettingsGroup[] = [
  {
    id: 'user',
    label: (_t, ts) => ts.settings.userSection,
    tabs: [
      DEFAULT_TAB,
      { id: 'privacy', label: (_t, ts) => ts.settings.privacy, Content: PrivacyTab },
      { id: 'security', label: (_t, ts) => ts.settings.security, Content: SecurityTab },
    ],
  },
  {
    id: 'app',
    label: (_t, ts) => ts.settings.appSection,
    tabs: [
      {
        id: 'appearance',
        label: (_t, ts) => ts.settings.appearance,
        Content: AppearanceTab,
      },
      {
        id: 'accessibility',
        label: (_t, ts) => ts.settings.accessibility,
        Content: AccessibilityTab,
      },
      {
        id: 'textMedia',
        label: (_t, ts) => ts.settings.textMedia,
        Content: TextMediaTab,
      },
      {
        id: 'language',
        label: (_t, ts) => ts.settings.languageAndTime,
        Content: LanguageTab,
      },
      {
        id: 'shortcuts',
        label: (_t, ts) => ts.settings.shortcuts,
        Content: ShortcutsTab,
      },
      { id: 'voice', label: (_t, ts) => ts.settings.voice, Content: VoiceTab },
      {
        id: 'notifications',
        label: (_t, ts) => ts.settings.notifications,
        Content: NotificationsTab,
      },
      { id: 'planning', label: (t) => t.planning.tabLabel, Content: PlanningTab },
      { id: 'system', label: (_t, ts) => ts.settings.system, Content: SystemTab },
      { id: 'updates', label: (t) => t.updates.title, Content: UpdatesTab },
      { id: 'advanced', label: (_t, ts) => ts.settings.advanced, Content: AdvancedTab },
    ],
  },
];

/** Retrouve un onglet par identifiant (repli : onglet par défaut). */
/** Casse + accents ignorés pour une recherche tolérante (« reglages » ⇒ « Réglages »). */
function fold(value: string): string {
  return value
    .toLowerCase()
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '');
}

/**
 * Filtre les groupes/onglets par libellé selon `query` (casse et accents
 * ignorés). Les groupes vidés disparaissent ; une requête vide rend tout.
 */
export function filterSettingsGroups(
  groups: SettingsGroup[],
  t: Dict,
  ts: SettingsDict,
  query: string,
): SettingsGroup[] {
  const needle = fold(query.trim());
  if (needle === '') return groups;
  return groups
    .map((group) => ({
      ...group,
      tabs: group.tabs.filter((tab) => fold(tab.label(t, ts)).includes(needle)),
    }))
    .filter((group) => group.tabs.length > 0);
}

export function findTab(id: SettingsTabId): SettingsTab {
  for (const group of SETTINGS_GROUPS) {
    const tab = group.tabs.find((candidate) => candidate.id === id);
    if (tab !== undefined) return tab;
  }
  return DEFAULT_TAB;
}
