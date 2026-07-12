/**
 * Bouton Soundboard du bandeau vocal : visible uniquement quand un salon vocal
 * de groupe est actif. Il déplie un petit panneau listant les sons du serveur
 * du salon actif (`state.sounds`). Un clic sur un son le joue localement
 * (feedback immédiat de l'émetteur) et demande au nœud de le diffuser aux
 * participants via `groups.soundboard.play`. Import de `playSound` = câblage du
 * gestionnaire `event.soundboard_play` au démarrage (voir `stores/soundboard`).
 */

import { useEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { ServerSound } from '../lib/api';
import { api } from '../lib/client';
import { useGroups } from '../stores/groups';
import { playSound } from '../stores/soundboard';
import { useUi, useT } from '../stores/ui';
import { useVoice } from '../stores/voice';

/** Icône haut-parleur du déclencheur (18 px, à l'unisson du bandeau vocal). */
function SoundboardIcon() {
  return (
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
      <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
      <path d="M15.54 8.46a5 5 0 0 1 0 7.07" />
      <path d="M19.07 4.93a10 10 0 0 1 0 14.14" />
    </svg>
  );
}

/**
 * `className` reprend le style des boutons d'action du bandeau vocal
 * (`ICON_BUTTON_CLASS`) pour rester visuellement homogène.
 */
export function SoundboardButton({ className }: { className: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const active = useVoice((s) => s.active);
  const sounds = useGroups((s) =>
    active === null ? undefined : s.states[active.groupId]?.sounds,
  );
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return undefined;
    const onDown = (e: MouseEvent): void => {
      if (wrapRef.current !== null && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') setOpen(false);
    };
    window.addEventListener('mousedown', onDown);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', onDown);
      window.removeEventListener('keydown', onKey);
    };
  }, [open]);

  // Le bandeau vocal ne rend ce bouton qu'en vocal de groupe ; garde défensive.
  if (active === null) return null;

  const list = sounds ?? [];

  const jouer = (sound: ServerSound): void => {
    // Feedback immédiat local : le clip est déjà en état, aucune source à viser.
    playSound(sound.merkle_root);
    api
      .groupsSoundboardPlay(active.groupId, active.channelId, sound.name)
      .catch(() => toast('error', t.errors.actionFailed));
  };

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        aria-label={t.soundboard.open}
        title={t.soundboard.open}
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
        className={`${className} ${open ? 'text-norm' : 'text-muted hover:text-norm'}`}
      >
        <SoundboardIcon />
      </button>
      {open && (
        <div
          role="menu"
          aria-label={t.soundboard.open}
          className="absolute bottom-full right-0 z-50 mb-2 w-60 rounded-lg border border-[color:var(--glass-border)] bg-chat p-2 shadow-3"
        >
          {list.length === 0 ? (
            <p className="px-2 py-3 text-xs text-faint">{t.soundboard.panelEmpty}</p>
          ) : (
            <div className="grid max-h-64 grid-cols-2 gap-1 overflow-y-auto">
              {list.map((sound) => (
                <button
                  key={sound.name}
                  type="button"
                  role="menuitem"
                  title={interpolate(t.soundboard.playOf, { name: sound.name })}
                  onClick={() => jouer(sound)}
                  className="truncate rounded-md bg-sidebar px-2 py-1.5 text-left font-mono text-xs font-medium text-norm transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
                >
                  {sound.name}
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
