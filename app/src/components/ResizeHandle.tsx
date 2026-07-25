/**
 * Poignée de redimensionnement façon Discord : fine zone de saisie verticale
 * (~6 px) posée sur le bord entre deux colonnes, glissée à la souris/au
 * tactile (Pointer Events, sans écouteur global — voir `stopDragging`) ou
 * ajustée au clavier (flèches, Origine/Fin). Un double-clic restaure la
 * largeur par défaut. Purement contrôlée : le parent fournit `value`/`min`/
 * `max`/`defaultValue` et reçoit les nouvelles largeurs via `onChange` — la
 * source de vérité (bornes, persistance) reste le store appelant.
 */

import { useCallback, useRef, useState } from 'react';

/** Pas d'ajustement au clavier (flèches), en pixels. */
const KEYBOARD_STEP_PX = 8;

export interface ResizeHandleProps {
  /** Largeur actuelle du panneau contrôlé (px). */
  value: number;
  min: number;
  max: number;
  /** Largeur restaurée par un double-clic sur la poignée. */
  defaultValue: number;
  /** Reçoit la nouvelle largeur, déjà bornée à `[min, max]`. */
  onChange: (value: number) => void;
  ariaLabel: string;
  /**
   * Côté de la poignée où vit le panneau redimensionné : `'left'` quand le
   * panneau précède la poignée (ex. barre latérale — glisser vers la droite
   * l'agrandit), `'right'` quand il la suit (ex. liste des membres — glisser
   * vers la droite la réduit).
   */
  panelSide: 'left' | 'right';
  /** Classe `ring-offset-*` assortie à la surface sous la poignée. */
  ringOffsetClassName?: string;
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function ResizeHandle({
  value,
  min,
  max,
  defaultValue,
  onChange,
  ariaLabel,
  panelSide,
  ringOffsetClassName = 'ring-offset-chat',
}: ResizeHandleProps) {
  const [isDragging, setIsDragging] = useState(false);
  /** Point de départ du glissé courant ; `null` hors glissé. */
  const dragOrigin = useRef<{ startX: number; startValue: number } | null>(null);
  /** `user-select` du document avant le glissé, à restaurer en le terminant. */
  const previousUserSelect = useRef<string | null>(null);

  /** Convertit un déplacement horizontal en nouvelle largeur bornée. */
  const applyDelta = useCallback(
    (deltaPx: number, fromValue: number) => {
      const signedDelta = panelSide === 'left' ? deltaPx : -deltaPx;
      onChange(clamp(fromValue + signedDelta, min, max));
    },
    [onChange, min, max, panelSide],
  );

  const stopDragging = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    if (dragOrigin.current === null) return;
    dragOrigin.current = null;
    setIsDragging(false);
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    document.body.style.userSelect = previousUserSelect.current ?? '';
    previousUserSelect.current = null;
  }, []);

  const onPointerDown = (e: React.PointerEvent<HTMLDivElement>): void => {
    if (e.button !== 0) return;
    dragOrigin.current = { startX: e.clientX, startValue: value };
    setIsDragging(true);
    e.currentTarget.setPointerCapture?.(e.pointerId);
    previousUserSelect.current = document.body.style.userSelect;
    document.body.style.userSelect = 'none';
  };

  const onPointerMove = (e: React.PointerEvent<HTMLDivElement>): void => {
    const origin = dragOrigin.current;
    if (origin === null) return;
    applyDelta(e.clientX - origin.startX, origin.startValue);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    switch (e.key) {
      case 'ArrowLeft':
        e.preventDefault();
        applyDelta(-KEYBOARD_STEP_PX, value);
        break;
      case 'ArrowRight':
        e.preventDefault();
        applyDelta(KEYBOARD_STEP_PX, value);
        break;
      case 'Home':
        e.preventDefault();
        onChange(min);
        break;
      case 'End':
        e.preventDefault();
        onChange(max);
        break;
      default:
        break;
    }
  };

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-label={ariaLabel}
      aria-valuenow={Math.round(value)}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={stopDragging}
      onPointerCancel={stopDragging}
      onDoubleClick={() => onChange(defaultValue)}
      onKeyDown={onKeyDown}
      className={`group relative z-10 w-1.5 shrink-0 select-none touch-none cursor-col-resize focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 ${ringOffsetClassName}`}
    >
      {/*
       * Aucun indicateur visuel au survol (retour utilisateur : la ligne
       * bleue était intrusive) — le curseur col-resize suffit. Un fin
       * liseré neutre n'apparaît que PENDANT le glissement, comme repère.
       */}
      <span
        aria-hidden
        className={`pointer-events-none absolute inset-y-0 start-1/2 w-px -translate-x-1/2 bg-norm/20 transition-opacity duration-fast ${
          isDragging ? 'opacity-100' : 'opacity-0'
        }`}
      />
    </div>
  );
}
