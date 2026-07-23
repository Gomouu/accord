/**
 * Catalogue des onglets de paramètres : ajouter un onglet se résume à une
 * entrée dans `SETTINGS_GROUPS` (libellé i18n + composant de contenu).
 */

import type { ComponentType } from 'react';
import type { Dict } from '../../i18n';
import { AccessibilityTab } from './AccessibilityTab';
import { AccountTab } from './AccountTab';
import { AdvancedTab } from './AdvancedTab';
import { AppearanceTab } from './AppearanceTab';
import { LanguageTab } from './LanguageTab';
import { NotificationsTab } from './NotificationsTab';
import { PlanningTab } from './PlanningTab';
import { PrivacyTab } from './PrivacyTab';
import { ShortcutsTab } from './ShortcutsTab';
import { SystemTab } from './SystemTab';
import { TextMediaTab } from './TextMediaTab';
import { UpdatesTab } from './UpdatesTab';
import { VoiceTab } from './VoiceTab';

export type SettingsTabId =
  | 'account'
  | 'privacy'
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
  label: (t: Dict) => string;
  Content: ComponentType;
}

export interface SettingsGroup {
  id: string;
  label: (t: Dict) => string;
  tabs: SettingsTab[];
}

/** Onglet ouvert par défaut. */
export const DEFAULT_TAB: SettingsTab = {
  id: 'account',
  label: (t) => t.settings.account,
  Content: AccountTab,
};

export const SETTINGS_GROUPS: SettingsGroup[] = [
  {
    id: 'user',
    label: (t) => t.settings.userSection,
    tabs: [
      DEFAULT_TAB,
      { id: 'privacy', label: (t) => t.settings.privacy, Content: PrivacyTab },
    ],
  },
  {
    id: 'app',
    label: (t) => t.settings.appSection,
    tabs: [
      { id: 'appearance', label: (t) => t.settings.appearance, Content: AppearanceTab },
      {
        id: 'accessibility',
        label: (t) => t.settings.accessibility,
        Content: AccessibilityTab,
      },
      { id: 'textMedia', label: (t) => t.settings.textMedia, Content: TextMediaTab },
      {
        id: 'language',
        label: (t) => t.settings.languageAndTime,
        Content: LanguageTab,
      },
      { id: 'shortcuts', label: (t) => t.settings.shortcuts, Content: ShortcutsTab },
      { id: 'voice', label: (t) => t.settings.voice, Content: VoiceTab },
      {
        id: 'notifications',
        label: (t) => t.settings.notifications,
        Content: NotificationsTab,
      },
      { id: 'planning', label: (t) => t.planning.tabLabel, Content: PlanningTab },
      { id: 'system', label: (t) => t.settings.system, Content: SystemTab },
      { id: 'updates', label: (t) => t.updates.title, Content: UpdatesTab },
      { id: 'advanced', label: (t) => t.settings.advanced, Content: AdvancedTab },
    ],
  },
];

/** Retrouve un onglet par identifiant (repli : onglet par défaut). */
export function findTab(id: SettingsTabId): SettingsTab {
  for (const group of SETTINGS_GROUPS) {
    const tab = group.tabs.find((candidate) => candidate.id === id);
    if (tab !== undefined) return tab;
  }
  return DEFAULT_TAB;
}
