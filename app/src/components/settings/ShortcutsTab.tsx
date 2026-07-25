/**
 * Onglet Raccourcis clavier : aide-mémoire complet, rendu depuis le catalogue
 * de `lib/shortcuts.ts` — purement informatif, aucun réglage ici. L'appui-pour-
 * parler s'y ajoute avec la touche réellement configurée, puisqu'elle varie.
 * `⌘` s'affiche sur macOS (`navigator.platform`), `Ctrl` ailleurs.
 */

import { formatKeyLabel } from '../../hooks/usePushToTalk';
import { isMacPlatform } from '../../lib/quickSwitch';
import {
  SHORTCUTS,
  SHORTCUT_SECTIONS,
  renderCombo,
  type ShortcutSection,
} from '../../lib/shortcuts';
import { useT, useUi } from '../../stores/ui';
import { SettingsSection } from './controls';

/** Une combinaison de touches, rendue en pastilles façon clavier. */
function Kbd({ keys }: { keys: string[] }) {
  return (
    <span className="flex shrink-0 items-center gap-1">
      {keys.map((key, i) => (
        <kbd
          key={i}
          className="rounded-sm border border-rail bg-input px-1.5 py-0.5 font-mono text-[11px] font-medium text-norm"
        >
          {key}
        </kbd>
      ))}
    </span>
  );
}

function ShortcutRow({ label, keys }: { label: string; keys: string[] }) {
  return (
    <div className="mb-1.5 flex items-center justify-between gap-4 rounded-lg bg-sidebar px-4 py-2.5">
      <span className="min-w-0 text-sm text-norm">{label}</span>
      <Kbd keys={keys} />
    </div>
  );
}

export function ShortcutsTab() {
  const t = useT();
  const pttEnabled = useUi((s) => s.pttEnabled);
  const pttKey = useUi((s) => s.pttKey);
  const mod = isMacPlatform() ? '⌘' : 'Ctrl';
  const opts = { mod, enter: t.shortcuts.keyEnter, esc: t.app.escKey };

  const rowsOf = (section: ShortcutSection) =>
    SHORTCUTS.filter((s) => s.section === section).map((s) => (
      <ShortcutRow
        key={s.key}
        label={t.shortcuts[s.key]}
        keys={renderCombo(s.combo, opts)}
      />
    ));

  return (
    <div>
      {SHORTCUT_SECTIONS.map(({ id, titleKey }) => (
        <SettingsSection key={id} title={t.shortcuts[titleKey]}>
          {rowsOf(id)}
          {id === 'voice' && (
            <ShortcutRow
              label={t.shortcuts.pushToTalkLabel}
              keys={[pttEnabled ? formatKeyLabel(pttKey) : t.shortcuts.pushToTalkOff]}
            />
          )}
        </SettingsSection>
      ))}
    </div>
  );
}
