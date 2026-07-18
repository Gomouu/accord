/**
 * Bouton Soundboard du bandeau vocal : visible uniquement quand un salon vocal
 * de groupe est actif. Il déplie un panneau listant les sons du serveur du
 * salon actif (`state.sounds`) en tuiles : pastille d'initiale à teinte stable
 * (dérivée du nom, `lib/color.hueFromString`), nom lisible, retour visuel de
 * lecture (pulsation brève) et recherche au-delà de quelques sons. Un clic
 * sur une tuile joue le son localement (feedback immédiat de l'émetteur) et
 * demande au nœud de le diffuser aux participants via
 * `groups.soundboard.play`. Import de `playSound` = câblage du gestionnaire
 * `event.soundboard_play` au démarrage (voir `stores/soundboard`).
 */

import { useEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { ServerSound } from '../lib/api';
import { api } from '../lib/client';
import { soundBadgeColor } from '../lib/color';
import { bouclerTab, focusables } from '../lib/focus';
import { hasPerm, PERMISSIONS, useGroups } from '../stores/groups';
import { playSound } from '../stores/soundboard';
import { useUi, useT } from '../stores/ui';
import { useVoice } from '../stores/voice';

/** Durée du retour visuel « en lecture » sur une tuile (ms). */
const PLAYING_PULSE_MS = 900;
/** Au-delà de ce nombre de sons, le panneau affiche un champ de recherche. */
const SEARCH_THRESHOLD = 8;

/**
 * Icône du déclencheur : grille de pads façon launchpad (3 × 2), lisible
 * comme une « planche de sons » là où un haut-parleur se confondait avec le
 * réglage de volume. `size` permet de la réutiliser en petit dans l'en-tête
 * du panneau.
 */
function SoundboardIcon({ size = 18 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinejoin="round"
      aria-hidden
    >
      <rect x="4" y="5" width="4" height="5.5" rx="1.2" />
      <rect x="10" y="5" width="4" height="5.5" rx="1.2" />
      <rect x="16" y="5" width="4" height="5.5" rx="1.2" />
      <rect x="4" y="13.5" width="4" height="5.5" rx="1.2" />
      <rect x="10" y="13.5" width="4" height="5.5" rx="1.2" />
      <rect x="16" y="13.5" width="4" height="5.5" rx="1.2" />
    </svg>
  );
}

/**
 * Pad d'un son façon launchpad : face carrée à teinte stable dérivée du nom
 * (`lib/color.soundBadgeColor`) portant l'initiale, nom lisible en dessous.
 * Survol = légère élévation (scale + ombre), clic = écrasement, lecture =
 * halo pulsé — uniquement transform/opacity/box-shadow (compositor). Le nom
 * accessible du bouton vient de son texte (le nom du son) ; la face est
 * décorative (`aria-hidden`).
 */
function SoundTile({
  sound,
  playing,
  onPlay,
}: {
  sound: ServerSound;
  playing: boolean;
  onPlay: (sound: ServerSound) => void;
}) {
  const t = useT();
  const initial = sound.name.charAt(0).toUpperCase();
  return (
    <button
      type="button"
      title={interpolate(t.soundboard.playOf, { name: sound.name })}
      onClick={() => onPlay(sound)}
      className="group flex flex-col gap-1 rounded-lg p-1 text-center transition-transform duration-fast active:scale-95 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
    >
      <span
        aria-hidden
        style={{ backgroundColor: soundBadgeColor(sound.name) }}
        className={`flex aspect-square w-full items-center justify-center rounded-lg text-lg font-bold text-white shadow-1 transition-[transform,box-shadow] duration-fast group-hover:scale-[1.03] group-hover:shadow-2 ${
          playing ? 'animate-pulse ring-2 ring-white/70' : ''
        }`}
      >
        {initial}
      </span>
      <span className="w-full truncate text-[11px] font-medium text-muted group-hover:text-norm">
        {sound.name}
      </span>
    </button>
  );
}

/**
 * `className` reprend le style des boutons d'action du bandeau vocal
 * (`ICON_BUTTON_CLASS`) pour rester visuellement homogène.
 */
export function SoundboardButton({ className }: { className: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const openModal = useUi((s) => s.openModal);
  const active = useVoice((s) => s.active);
  const groupState = useGroups((s) =>
    active === null ? undefined : s.states[active.groupId],
  );
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [playingName, setPlayingName] = useState<string | null>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const pulseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (!open) return undefined;
    // Focus déplacé dans le panneau (recherche ou première tuile) : les
    // contrôles sont immédiatement au clavier, Échap rend le focus au bouton.
    focusables(panelRef.current)[0]?.focus();
    const onDown = (e: MouseEvent): void => {
      if (wrapRef.current !== null && !wrapRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        setOpen(false);
        triggerRef.current?.focus();
      } else if (e.key === 'Tab') {
        // Piège à focus sur l'enveloppe : le cycle couvre déclencheur + panneau.
        bouclerTab(e, wrapRef.current);
      }
    };
    window.addEventListener('mousedown', onDown);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', onDown);
      window.removeEventListener('keydown', onKey);
    };
  }, [open]);

  // Nettoyage du minuteur de pulsation au démontage.
  useEffect(
    () => () => {
      if (pulseTimer.current !== null) clearTimeout(pulseTimer.current);
    },
    [],
  );

  // Le bandeau vocal ne rend ce bouton qu'en vocal de groupe ; garde défensive.
  if (active === null) return null;

  const list = groupState?.sounds ?? [];
  const canManage =
    groupState !== undefined &&
    hasPerm(groupState.my_permissions, PERMISSIONS.MANAGE_EMOJIS);
  const showSearch = list.length > SEARCH_THRESHOLD;
  const needle = query.trim().toLowerCase();
  const filtered = needle === '' ? list : list.filter((s) => s.name.includes(needle));

  const jouer = (sound: ServerSound): void => {
    // Retour visuel immédiat, borné : la pulsation s'éteint d'elle-même.
    setPlayingName(sound.name);
    if (pulseTimer.current !== null) clearTimeout(pulseTimer.current);
    pulseTimer.current = setTimeout(() => setPlayingName(null), PLAYING_PULSE_MS);
    // Feedback local : le clip est déjà en état (préchargé), aucune source à
    // viser ; l'échec de lecture est signalé par le store (toast).
    void playSound(sound.merkle_root);
    api
      .groupsSoundboardPlay(active.groupId, active.channelId, sound.name)
      .catch(() => toast('error', t.errors.actionFailed));
  };

  const ouvrirReglages = (): void => {
    setOpen(false);
    // Focus rendu au déclencheur avant la modale : c'est lui que la modale
    // mémorise, et donc lui qui récupère le focus à sa fermeture.
    triggerRef.current?.focus();
    openModal({
      kind: 'serverSettings',
      groupId: active.groupId,
      initialTab: 'soundboard',
    });
  };

  return (
    <div ref={wrapRef} className="relative">
      <button
        ref={triggerRef}
        type="button"
        aria-label={t.soundboard.open}
        title={t.soundboard.open}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => {
          setQuery('');
          setOpen((o) => !o);
        }}
        className={`${className} ${open ? 'text-norm' : 'text-muted hover:text-norm'}`}
      >
        <SoundboardIcon />
      </button>
      {open && (
        <div
          ref={panelRef}
          role="dialog"
          aria-label={t.soundboard.open}
          className="glass-strong liquid-floating popover-enter absolute bottom-full right-0 z-50 mb-2 w-72 rounded-lg p-2"
        >
          <div className="flex items-center justify-between px-1 pb-2">
            <span className="flex items-center gap-1.5 text-xs font-semibold uppercase tracking-wide text-muted">
              <SoundboardIcon size={14} />
              {t.soundboard.open}
            </span>
            {list.length > 0 && (
              <span className="rounded-full bg-sidebar px-1.5 py-0.5 text-[11px] font-semibold tabular-nums text-faint">
                {list.length}
              </span>
            )}
          </div>
          {showSearch && (
            <input
              aria-label={t.soundboard.searchPlaceholder}
              placeholder={t.soundboard.searchPlaceholder}
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              className="mb-1.5 w-full rounded-md border border-transparent bg-input px-2 py-1 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
            />
          )}
          {list.length === 0 ? (
            <div className="flex flex-col items-center gap-1.5 px-2 py-5 text-center text-faint">
              <SoundboardIcon size={24} />
              <p className="text-xs">{t.soundboard.panelEmpty}</p>
              {canManage && (
                <button
                  type="button"
                  onClick={ouvrirReglages}
                  className="mt-2 text-xs font-medium text-link hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
                >
                  {t.soundboard.openSettings}
                </button>
              )}
            </div>
          ) : filtered.length === 0 ? (
            <p className="px-2 py-4 text-center text-xs text-faint">
              {t.soundboard.noResults}
            </p>
          ) : (
            <div className="grid max-h-72 grid-cols-3 gap-2 overflow-y-auto p-0.5">
              {filtered.map((sound) => (
                <SoundTile
                  key={sound.name}
                  sound={sound}
                  playing={playingName === sound.name}
                  onPlay={jouer}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
