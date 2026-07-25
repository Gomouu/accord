/**
 * Réactions d'un message : agrégation des paires emoji × auteur en pastilles
 * (emoji, compte, réaction de l'utilisateur) et rangée cliquable sous le
 * corps du message, à la Discord. Un clic droit sur une pastille ouvre un
 * popover listant les auteurs ayant réagi avec cet emoji (avatars + noms).
 */

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { Reaction } from '../lib/api';
import { jetonEmojiTexte, nomReactionEmoji } from '../lib/emoji';
import { useT } from '../stores/ui';
import { Avatar } from './Avatar';
import { CustomEmoji } from './CustomEmoji';

/** Pastille agrégée : un emoji, son compte et si l'utilisateur a réagi. */
export interface ReactionPill {
  emoji: string;
  count: number;
  mine: boolean;
}

/**
 * Agrège les réactions brutes en pastilles, dans l'ordre de première
 * apparition de chaque emoji (stable d'un rendu à l'autre).
 */
export function aggregateReactions(
  reactions: readonly Reaction[] | undefined,
  selfPubkey: string | null,
): ReactionPill[] {
  const pills = new Map<string, ReactionPill>();
  for (const { emoji, author } of reactions ?? []) {
    const pill = pills.get(emoji) ?? { emoji, count: 0, mine: false };
    pills.set(emoji, {
      ...pill,
      count: pill.count + 1,
      mine: pill.mine || author === selfPubkey,
    });
  }
  return [...pills.values()];
}

/**
 * Auteurs ayant réagi avec `emoji`, dans l'ordre de première apparition et
 * dédupliqués (un même auteur n'apparaît qu'une fois par emoji).
 */
export function reactorsOf(
  reactions: readonly Reaction[] | undefined,
  emoji: string,
): string[] {
  const seen = new Set<string>();
  const authors: string[] = [];
  for (const { emoji: e, author } of reactions ?? []) {
    if (e === emoji && !seen.has(author)) {
      seen.add(author);
      authors.push(author);
    }
  }
  return authors;
}

/** Affichage d'une valeur de réaction : `:name:` pour un custom, sinon l'emoji. */
function displayOf(emoji: string): string {
  const nom = nomReactionEmoji(emoji);
  return nom !== null ? jetonEmojiTexte(nom) : emoji;
}

/** Largeur du popover « qui a réagi » (px) ; sert au calcul de position. */
const POPOVER_WIDTH = 208;
/** Marge minimale au bord du viewport (px). */
const POPOVER_MARGE = 8;

interface ReactorsPopoverProps {
  /** Libellé accessible déjà interpolé (« Ont réagi avec 👍 »). */
  label: string;
  authors: readonly string[];
  /** Rectangle de la pastille déclencheuse (ancrage). */
  anchor: DOMRect;
  nameOf: (pubkey: string) => string;
  avatarHashOf: (pubkey: string) => string | null;
  avatarDecorationOf: (pubkey: string) => string | null;
  /** Ouvre la carte de profil d'un auteur (optionnel). */
  onOpenAuthor?: ((pubkey: string, target: HTMLElement) => void) | undefined;
  onClose: () => void;
}

/** Popover ancré listant les auteurs d'une réaction ; ferme au clic extérieur/Échap. */
function ReactorsPopover({
  label,
  authors,
  anchor,
  nameOf,
  avatarHashOf,
  avatarDecorationOf,
  onOpenAuthor,
  onClose,
}: ReactorsPopoverProps) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  // Position calée après mesure réelle (hauteur variable), bornée au viewport.
  useLayoutEffect(() => {
    if (ref.current === null) return;
    const h = ref.current.offsetHeight;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    const left = Math.max(
      POPOVER_MARGE,
      Math.min(anchor.left, vw - POPOVER_WIDTH - POPOVER_MARGE),
    );
    const enDessous = anchor.bottom + POPOVER_MARGE + h <= vh;
    const top = enDessous
      ? anchor.bottom + POPOVER_MARGE
      : Math.max(POPOVER_MARGE, anchor.top - POPOVER_MARGE - h);
    setPos({ left, top });
  }, [anchor]);

  useEffect(() => {
    ref.current?.focus();
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    const onDown = (e: MouseEvent): void => {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onDown);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label={label}
      tabIndex={-1}
      style={{
        position: 'fixed',
        left: pos?.left ?? anchor.left,
        top: pos?.top ?? anchor.bottom,
        width: POPOVER_WIDTH,
        visibility: pos === null ? 'hidden' : 'visible',
      }}
      className="glass-strong popover-enter z-50 max-h-64 overflow-y-auto rounded-lg p-1.5 focus:outline-none"
    >
      <h4 className="mb-1 px-1.5 pt-0.5 text-xs font-medium uppercase tracking-wide text-faint">
        {label}
      </h4>
      <ul className="flex flex-col">
        {authors.map((pubkey) => {
          const name = nameOf(pubkey);
          const contenu = (
            <>
              <Avatar
                id={pubkey}
                name={name}
                size={24}
                avatarHash={avatarHashOf(pubkey)}
                hint={pubkey}
                decoration={avatarDecorationOf(pubkey)}
              />
              <span className="truncate text-sm text-norm">{name}</span>
            </>
          );
          return (
            <li key={pubkey}>
              {onOpenAuthor !== undefined ? (
                <button
                  type="button"
                  onClick={(e) => onOpenAuthor(pubkey, e.currentTarget)}
                  className="flex w-full items-center gap-2 rounded-md px-1.5 py-1 text-start transition-colors duration-fast hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none"
                >
                  {contenu}
                </button>
              ) : (
                <div className="flex items-center gap-2 px-1.5 py-1">{contenu}</div>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

interface ReactionRowProps {
  reactions: readonly Reaction[];
  selfPubkey: string | null;
  /** Bascule sa propre réaction ; absent = pastilles en lecture seule. */
  onToggle?: ((emoji: string) => void) | undefined;
  /** Émojis du serveur (nom → racine Merkle) pour les réactions custom. */
  emojis?: ReadonlyMap<string, string> | undefined;
  /** Pair source probable pour le téléchargement des images d'émoji. */
  hint?: string | undefined;
  /** Résout le nom affichable d'un auteur (popover « qui a réagi »). */
  nameOf?: ((pubkey: string) => string) | undefined;
  /** Résout le hash d'avatar d'un auteur (popover « qui a réagi »). */
  avatarHashOf?: ((pubkey: string) => string | null) | undefined;
  /** Résout la décoration d'avatar d'un auteur (popover « qui a réagi »). */
  avatarDecorationOf?: ((pubkey: string) => string | null) | undefined;
  /** Ouvre la carte de profil d'un auteur depuis le popover (optionnel). */
  onOpenAuthor?: ((pubkey: string, target: HTMLElement) => void) | undefined;
}

/** Rangée de pastilles sous un message ; rendue seulement si non vide. */
export function ReactionRow({
  reactions,
  selfPubkey,
  onToggle,
  emojis,
  hint,
  nameOf,
  avatarHashOf,
  avatarDecorationOf,
  onOpenAuthor,
}: ReactionRowProps) {
  const t = useT();
  const pills = aggregateReactions(reactions, selfPubkey);
  // Popover « qui a réagi » : emoji ciblé + rectangle de la pastille cliquée.
  const [popover, setPopover] = useState<{ emoji: string; anchor: DOMRect } | null>(null);
  if (pills.length === 0) return null;

  const resolveName = nameOf ?? ((pubkey: string) => pubkey.slice(0, 6));
  const resolveAvatar = avatarHashOf ?? (() => null);
  const resolveDecoration = avatarDecorationOf ?? (() => null);

  return (
    <>
      <div className="mt-1 flex flex-wrap gap-1.5">
        {pills.map((pill) => {
          // Réaction custom `":name:"` : image si l'émoji est connu du serveur.
          const nom = nomReactionEmoji(pill.emoji);
          const merkle = nom !== null ? emojis?.get(nom) : undefined;
          const affichage = displayOf(pill.emoji);
          const label = interpolate(t.dm.reactWith, { emoji: affichage });
          return (
            <button
              key={pill.emoji}
              type="button"
              disabled={onToggle === undefined}
              aria-pressed={pill.mine}
              aria-label={label}
              title={label}
              onClick={() => onToggle?.(pill.emoji)}
              onContextMenu={(e) => {
                // Clic droit : liste des auteurs (jamais le menu du message).
                e.preventDefault();
                e.stopPropagation();
                setPopover({
                  emoji: pill.emoji,
                  anchor: e.currentTarget.getBoundingClientRect(),
                });
              }}
              className={`badge-pop flex h-7 items-center gap-1.5 rounded-full border px-2.5 text-sm transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat ${
                pill.mine
                  ? 'border-blurple bg-blurple/10 enabled:hover:border-blurple-hover'
                  : 'border-rail/70 bg-input enabled:hover:border-faint'
              }`}
            >
              {merkle !== undefined && nom !== null ? (
                <CustomEmoji name={nom} merkleRoot={merkle} hint={hint} size={16} />
              ) : (
                <span aria-hidden className="text-[13px] leading-none">
                  {affichage}
                </span>
              )}
              <span
                className={`text-xs font-semibold leading-none ${pill.mine ? 'text-norm' : 'text-muted'}`}
              >
                {pill.count}
              </span>
            </button>
          );
        })}
      </div>
      {popover !== null && (
        <ReactorsPopover
          label={interpolate(t.dm.whoReacted, { emoji: displayOf(popover.emoji) })}
          authors={reactorsOf(reactions, popover.emoji)}
          anchor={popover.anchor}
          nameOf={resolveName}
          avatarHashOf={resolveAvatar}
          avatarDecorationOf={resolveDecoration}
          onOpenAuthor={
            onOpenAuthor === undefined
              ? undefined
              : (pubkey, target) => {
                  onOpenAuthor(pubkey, target);
                  setPopover(null);
                }
          }
          onClose={() => setPopover(null)}
        />
      )}
    </>
  );
}
