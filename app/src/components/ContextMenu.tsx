/**
 * Rendu du menu contextuel générique (voir `stores/contextMenu.ts`) : ouvert
 * au clic droit sur une surface prise en charge (message, utilisateur, salon,
 * serveur), positionné au curseur et borné au viewport — même schéma que
 * `ProfilePopover` (mesure réelle puis repositionnement). Se ferme au clic
 * extérieur, à Échap, au défilement d'un conteneur quelconque, ou à la perte
 * de focus de la fenêtre. Navigation clavier : flèches haut/bas déplacent le
 * focus (roving tabindex), Entrée active l'item courant.
 *
 * Exporte aussi le petit jeu d'icônes partagé par les différents menus
 * (message, utilisateur, salon, serveur) pour rester visuellement cohérent
 * sans dupliquer les tracés SVG à chaque site d'appel.
 */

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import {
  useContextMenu,
  type ContextMenuAnchor,
  type ContextMenuItem,
} from '../stores/contextMenu';
import type { NotifLevel } from '../stores/mute';

/** Marge minimale au bord du viewport (px), comme `ProfilePopover`. */
const MARGE = 8;

/** Position `fixed` (px) bornée au viewport, calée près du point de clic. */
export function placeContextMenu({
  x,
  y,
  width,
  height,
  viewportWidth,
  viewportHeight,
  anchor,
  preferredSide = 'top',
  gap = MARGE,
}: {
  x: number;
  y: number;
  width: number;
  height: number;
  viewportWidth: number;
  viewportHeight: number;
  anchor?: ContextMenuAnchor | undefined;
  preferredSide?: 'top' | 'bottom' | undefined;
  gap?: number | undefined;
}): {
  left: number;
  top: number;
  side: 'point' | 'top' | 'bottom';
  maxHeight: number | null;
} {
  let left = x;
  let top = y;
  let side: 'point' | 'top' | 'bottom' = 'point';
  const viewportSpace = Math.max(0, viewportHeight - MARGE * 2);
  let effectiveHeight = Math.min(height, viewportSpace);
  let maxHeight = height > viewportSpace ? viewportSpace : null;

  if (anchor !== undefined) {
    left = anchor.left;
    const aboveSpace = Math.max(0, anchor.top - gap - MARGE);
    const belowSpace = Math.max(0, viewportHeight - anchor.bottom - gap - MARGE);
    const fitsAbove = height <= aboveSpace;
    const fitsBelow = height <= belowSpace;
    const useAbove =
      preferredSide === 'top'
        ? fitsAbove || (!fitsBelow && aboveSpace >= belowSpace)
        : !fitsBelow && (fitsAbove || aboveSpace > belowSpace);
    const available = useAbove ? aboveSpace : belowSpace;
    effectiveHeight = Math.min(height, available);
    maxHeight = height > available ? available : null;

    if (useAbove) {
      top = anchor.top - gap - effectiveHeight;
      side = 'top';
    } else {
      top = anchor.bottom + gap;
      side = 'bottom';
    }
  }

  return {
    left: Math.max(MARGE, Math.min(left, viewportWidth - width - MARGE)),
    top: Math.max(MARGE, Math.min(top, viewportHeight - effectiveHeight - MARGE)),
    side,
    maxHeight,
  };
}

export function ContextMenu() {
  const menu = useContextMenu((s) => s.menu);
  const closeMenu = useContextMenu((s) => s.closeMenu);
  const ref = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  /** Élément focusé avant l'ouverture (ligne cliquée/menu clavier), pour restauration. */
  const declencheurRef = useRef<HTMLElement | null>(null);
  const [pos, setPos] = useState<{
    left: number;
    top: number;
    side: 'point' | 'top' | 'bottom';
    maxHeight: number | null;
  } | null>(null);
  const [activeIndex, setActiveIndex] = useState(0);

  // Repositionne après mesure réelle (largeur/hauteur variables selon les
  // items) et donne le focus au menu pour la navigation clavier. À la
  // fermeture, rend le focus au déclencheur — sauf s'il est déjà parti
  // ailleurs (clic sur un autre élément focusable).
  useLayoutEffect(() => {
    if (menu === null) {
      setPos(null);
      const declencheur = declencheurRef.current;
      declencheurRef.current = null;
      const actif = document.activeElement;
      if (
        declencheur !== null &&
        declencheur.isConnected &&
        (actif === null || actif === document.body || !actif.isConnected)
      ) {
        declencheur.focus();
      }
      return;
    }
    setActiveIndex(0);
    // Sous-menu (mêmes coordonnées, items remplacés) : le focus est déjà dans
    // le menu — on garde le déclencheur d'origine capturé à l'ouverture.
    const actif = document.activeElement;
    if (!(ref.current?.contains(actif) ?? false)) {
      declencheurRef.current = actif instanceof HTMLElement ? actif : null;
    }
    const el = ref.current;
    if (el === null) return;
    setPos(
      placeContextMenu({
        x: menu.x,
        y: menu.y,
        width: el.offsetWidth,
        height: el.offsetHeight,
        viewportWidth: window.innerWidth,
        viewportHeight: window.innerHeight,
        anchor: menu.anchor,
        preferredSide: menu.preferredSide,
        gap: menu.gap,
      }),
    );
    const focusFirst = (): void => {
      const first = itemRefs.current[0];
      if (first !== null && first !== undefined) first.focus();
      else el.focus();
    };
    focusFirst();
    queueMicrotask(() => {
      if (useContextMenu.getState().menu !== menu) return;
      focusFirst();
    });
  }, [menu]);

  useEffect(() => {
    if (menu === null) return undefined;
    const onDown = (e: MouseEvent): void => {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) closeMenu();
    };
    // Capture : un défilement dans n'importe quel conteneur (fil de
    // messages, liste de salons…) referme le menu — sa position au clic n'a
    // plus de sens une fois le contenu déplacé sous le curseur.
    const onScroll = (): void => closeMenu();
    window.addEventListener('mousedown', onDown);
    document.addEventListener('scroll', onScroll, true);
    window.addEventListener('blur', closeMenu);
    window.addEventListener('resize', closeMenu);
    window.visualViewport?.addEventListener('resize', closeMenu);
    return () => {
      window.removeEventListener('mousedown', onDown);
      document.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('blur', closeMenu);
      window.removeEventListener('resize', closeMenu);
      window.visualViewport?.removeEventListener('resize', closeMenu);
    };
  }, [menu, closeMenu]);

  if (menu === null) return null;
  const { items } = menu;

  const activate = (item: ContextMenuItem): void => {
    closeMenu();
    item.onClick();
  };

  const moveActive = (next: number): void => {
    const bounded = ((next % items.length) + items.length) % items.length;
    setActiveIndex(bounded);
    itemRefs.current[bounded]?.focus();
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeMenu();
    } else if (e.key === 'Tab') {
      // Convention `role="menu"` : Tab referme le menu au lieu d'envoyer le
      // focus derrière une surface encore ouverte.
      e.preventDefault();
      closeMenu();
    } else if (e.key === 'ArrowDown') {
      e.preventDefault();
      moveActive(activeIndex + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      moveActive(activeIndex - 1);
    } else if (e.key === 'Enter' || e.key === ' ') {
      const item = items[activeIndex];
      if (item !== undefined) {
        e.preventDefault();
        activate(item);
      }
    }
  };

  return (
    <div
      ref={ref}
      role="menu"
      tabIndex={-1}
      onKeyDown={onKeyDown}
      style={{
        position: 'fixed',
        left: pos?.left ?? menu.x,
        top: pos?.top ?? menu.y,
        maxHeight: pos?.maxHeight ?? undefined,
        visibility: pos === null ? 'hidden' : 'visible',
      }}
      className={`glass-strong context-menu-enter z-50 min-w-[210px] max-w-xs overflow-y-auto overscroll-contain rounded-lg p-1.5 focus:outline-none ${
        pos?.side === 'top' ? 'origin-bottom-left' : 'origin-top-left'
      }`}
    >
      {items.map((item, i) => (
        <div key={`${i}-${item.label}`}>
          {item.separatorBefore === true && (
            <div className="my-1.5 h-px bg-input/60" role="separator" />
          )}
          <button
            ref={(el) => {
              itemRefs.current[i] = el;
            }}
            type="button"
            role={item.checked === undefined ? 'menuitem' : 'menuitemradio'}
            aria-checked={item.checked}
            tabIndex={i === activeIndex ? 0 : -1}
            onMouseEnter={() => setActiveIndex(i)}
            onFocus={() => setActiveIndex(i)}
            onClick={() => activate(item)}
            className={`flex h-9 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-sm font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-blurple ${
              item.danger === true
                ? 'text-red hover:bg-red/10 focus-visible:bg-red/10'
                : 'text-norm hover:bg-chat-hover focus-visible:bg-chat-hover'
            }`}
          >
            {item.icon !== undefined && (
              <span
                aria-hidden
                className="flex h-[18px] w-[18px] shrink-0 items-center justify-center"
              >
                {item.icon}
              </span>
            )}
            <span className="min-w-0 flex-1 truncate">{item.label}</span>
            {item.checked === true && (
              <span
                aria-hidden
                className="flex h-[18px] w-[18px] shrink-0 items-center justify-center text-header"
              >
                <CheckMenuIcon />
              </span>
            )}
          </button>
        </div>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/* Sous-menu « Notifications » à trois niveaux (serveur/salon), partagé */
/* par `ServerRail` et `Sidebar` — voir `stores/mute.ts`.               */
/* ------------------------------------------------------------------ */

/** Ordre d'affichage des niveaux dans le sous-menu (du plus au moins bruyant). */
const NOTIF_LEVEL_ORDER: readonly NotifLevel[] = ['all', 'mentions', 'none'] as const;

/** Libellés i18n des trois niveaux (`t.notifLevel`). */
export interface NotifLevelLabels {
  all: string;
  mentions: string;
  none: string;
}

/**
 * Items du sous-menu de niveau de notification : un item radio par niveau, la
 * coche portée par le niveau `current` (effectif). `onPick` applique le choix.
 */
export function buildNotifLevelItems(
  labels: NotifLevelLabels,
  current: NotifLevel,
  onPick: (level: NotifLevel) => void,
): ContextMenuItem[] {
  return NOTIF_LEVEL_ORDER.map((level) => ({
    label: labels[level],
    checked: current === level,
    onClick: () => onPick(level),
  }));
}

/* ------------------------------------------------------------------ */
/* Jeu d'icônes partagé par les menus (message, utilisateur, salon,     */
/* serveur) — mêmes tracés que les boutons existants (MessageActions,   */
/* ChatView, Sidebar) pour rester visuellement cohérent.                */
/* ------------------------------------------------------------------ */

/** Attributs communs à chaque icône du set (voir ICON SPEC, styles/global.css). */
const ICON_PROPS = {
  width: 14,
  height: 14,
  viewBox: '0 0 24 24',
  fill: 'none',
  stroke: 'currentColor',
  strokeWidth: 2,
  strokeLinecap: 'round',
  strokeLinejoin: 'round',
  'aria-hidden': true,
} as const;

export function CopyMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <rect width="14" height="14" x="8" y="8" rx="2" />
      <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
    </svg>
  );
}

export function EditMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
      <path d="m15 5 4 4" />
    </svg>
  );
}

export function DeleteMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M3 6h18" />
      <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
      <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
      <line x1="10" x2="10" y1="11" y2="17" />
      <line x1="14" x2="14" y1="11" y2="17" />
    </svg>
  );
}

export function ReplyMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <polyline points="9 17 4 12 9 7" />
      <path d="M20 18v-2a4 4 0 0 0-4-4H4" />
    </svg>
  );
}

/** Case cochée : entrée en mode sélection de messages (modération). */
export function SelectMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M9 11l3 3L22 4" />
      <path d="M21 12v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h11" />
    </svg>
  );
}

export function ForwardMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <polyline points="15 17 20 12 15 7" />
      <path d="M4 18v-2a4 4 0 0 1 4-4h12" />
    </svg>
  );
}

export function PinMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <line x1="12" x2="12" y1="17" y2="22" />
      <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z" />
    </svg>
  );
}

/** Bulle de discussion : ouverture/création d'un fil sur un message. */
export function ThreadMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2Z" />
    </svg>
  );
}

export function CheckMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <polyline points="20 6 9 17 4 12" />
    </svg>
  );
}

export function ProfileMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2" />
      <circle cx="12" cy="7" r="4" />
    </svg>
  );
}

export function EnvelopeMenuIcon({ size = 14 }: { size?: number } = {}) {
  return (
    <svg {...ICON_PROPS} width={size} height={size}>
      <rect width="20" height="16" x="2" y="4" rx="2" />
      <path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7" />
    </svg>
  );
}

export function GearMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M10.3 3.6a2 2 0 0 1 3.4 0l.4.7a2 2 0 0 0 2.2.9l.8-.2a2 2 0 0 1 2.4 2.4l-.2.8a2 2 0 0 0 .9 2.2l.7.4a2 2 0 0 1 0 3.4l-.7.4a2 2 0 0 0-.9 2.2l.2.8a2 2 0 0 1-2.4 2.4l-.8-.2a2 2 0 0 0-2.2.9l-.4.7a2 2 0 0 1-3.4 0l-.4-.7a2 2 0 0 0-2.2-.9l-.8.2a2 2 0 0 1-2.4-2.4l.2-.8a2 2 0 0 0-.9-2.2l-.7-.4a2 2 0 0 1 0-3.4l.7-.4a2 2 0 0 0 .9-2.2l-.2-.8a2 2 0 0 1 2.4-2.4l.8.2a2 2 0 0 0 2.2-.9l.4-.7Z" />
      <circle cx="12" cy="12" r="3" />
    </svg>
  );
}

export function LeaveMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
      <polyline points="16 17 21 12 16 7" />
      <line x1="21" x2="9" y1="12" y2="12" />
    </svg>
  );
}

/** Icône « + » partagée (créer un salon, une catégorie…), 14 px du jeu de menu. */
export function PlusMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M5 12h14" />
      <path d="M12 5v14" />
    </svg>
  );
}

export function MentionMenuIcon() {
  return (
    <span aria-hidden className="text-xs font-medium leading-none">
      @
    </span>
  );
}

/** Icône de fermeture partagée (modales, popovers, panneaux plein écran). */
export function CloseIcon({ size = 16 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </svg>
  );
}

/** Icône de recherche partagée (barre de recherche, champs filtrants). */
export function SearchIcon({ size = 16 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <circle cx="11" cy="11" r="7" />
      <line x1="21" x2="16.65" y1="21" y2="16.65" />
    </svg>
  );
}

/** Icône téléphone partagée (bouton d'appel, décrocher un appel entrant). */
export function PhoneIcon({ size = 18 }: { size?: number } = {}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M13.832 16.568a1 1 0 0 0 1.213-.303l.355-.465A2 2 0 0 1 17 15h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2A18 18 0 0 1 2 4a2 2 0 0 1 2-2h3a2 2 0 0 1 2 2v3a2 2 0 0 1-.8 1.6l-.468.351a1 1 0 0 0-.292 1.233 14 14 0 0 0 6.392 6.384" />
    </svg>
  );
}

/** Icône téléphone barré partagée (raccrocher, refuser, annuler un appel). */
export function PhoneOffIcon({ size = 18 }: { size?: number } = {}) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M10.68 13.31a16 16 0 0 0 3.41 2.6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7 2 2 0 0 1 1.72 2v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.42 19.42 0 0 1-3.33-2.67m-2.67-3.34a19.79 19.79 0 0 1-3.07-8.63A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91" />
      <line x1="22" x2="2" y1="2" y2="22" />
    </svg>
  );
}

/** Icône micro barré du jeu de menu (14 px, modération vocale serveur). */
export function VoiceMuteMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <line x1="2" x2="22" y1="2" y2="22" />
      <path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2" />
      <path d="M5 10v2a7 7 0 0 0 12 5" />
      <path d="M15 9.34V5a3 3 0 0 0-5.68-1.33" />
      <path d="M9 9v3a3 3 0 0 0 5.12 2.12" />
      <line x1="12" x2="12" y1="19" y2="22" />
    </svg>
  );
}

/** Icône casque barré du jeu de menu (14 px, modération vocale serveur). */
export function VoiceDeafenMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M3 14h3a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H4a1 1 0 0 1-1-1v-4a9 9 0 0 1 18 0v4a1 1 0 0 1-1 1h-2a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2h3" />
      <line x1="2" x2="22" y1="2" y2="22" />
    </svg>
  );
}

/**
 * Icône cloche barrée du jeu de menu (14 px, sourdine des notifications d'un
 * serveur ou d'un salon — voir `stores/mute.ts`). Même tracé utilisé pour les
 * deux sens du bascule (mettre/retirer la sourdine) et comme indicateur
 * permanent sur une ligne de salon en sourdine (`Sidebar`) — seul le libellé
 * change selon l'état, comme `VoiceMuteMenuIcon` ci-dessus.
 */
export function BellOffMenuIcon() {
  return (
    <svg {...ICON_PROPS}>
      <path d="M8.7 3A6 6 0 0 1 18 8a21.3 21.3 0 0 0 .6 5" />
      <path d="M17 17H3s3-2 3-9c0-.5 0-1 .1-1.5" />
      <path d="M10.3 21a1.94 1.94 0 0 0 3.4 0" />
      <line x1="2" x2="22" y1="2" y2="22" />
    </svg>
  );
}
