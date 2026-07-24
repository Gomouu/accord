import { useUi } from '../stores/ui';

export type SettingsTabTarget = 'appearance' | 'voice' | 'privacy' | 'notifications';

let pendingTab: SettingsTabTarget | null = null;
const listeners = new Set<(tab: SettingsTabTarget) => void>();

export function openSettingsTab(tab: SettingsTabTarget): void {
  pendingTab = listeners.size === 0 ? tab : null;
  useUi.getState().openModal({ kind: 'settings' });
  listeners.forEach((listener) => listener(tab));
}

export function peekSettingsTab(): SettingsTabTarget | null {
  return pendingTab;
}

export function clearSettingsTab(tab: SettingsTabTarget): void {
  if (pendingTab === tab) pendingTab = null;
}

export function subscribeSettingsTab(
  listener: (tab: SettingsTabTarget) => void,
): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
