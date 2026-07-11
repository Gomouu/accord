/**
 * Sélecteur d'émojis en popover : émojis custom (du serveur courant quand un
 * `groupId` est fourni, sinon agrégés de tous les serveurs rejoints en MP)
 * puis un jeu d'émojis Unicode courants, groupés et filtrables. Se ferme au
 * clic extérieur et à Échap ; le champ de recherche prend le focus.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import {
  EMOJIS_UNICODE,
  jetonEmojiTexte,
  type EmojiPick,
  type EmojiUnicode,
} from '../lib/emoji';
import { aggregateEmojis, useGroups, type AggregatedEmoji } from '../stores/groups';
import { interpolate } from '../i18n';
import type { Dict } from '../i18n';
import { useT } from '../stores/ui';
import { CustomEmoji } from './CustomEmoji';

interface EmojiPickerProps {
  /** Contexte serveur : expose ses émojis custom (`null`/absent = MP). */
  groupId?: string | null;
  onSelect: (pick: EmojiPick) => void;
  onClose: () => void;
  /** Classes de placement du panneau (positionné par l'appelant). */
  positionClass?: string;
}

/** Libellé i18n d'une catégorie Unicode. */
function labelCategorie(id: string, t: Dict): string {
  const labels: Record<string, string> = {
    smileys: t.emoji.catSmileys,
    gestures: t.emoji.catGestures,
    hearts: t.emoji.catHearts,
    animals: t.emoji.catAnimals,
    food: t.emoji.catFood,
    activities: t.emoji.catActivities,
    objects: t.emoji.catObjects,
    symbols: t.emoji.catSymbols,
  };
  return labels[id] ?? id;
}

/** Vrai si l'émoji Unicode correspond à la recherche (mots-clés ou caractère). */
function correspond(emoji: EmojiUnicode, q: string): boolean {
  if (q === '') return true;
  if (emoji.char.includes(q)) return true;
  return emoji.keywords.some((k) => k.includes(q));
}

export function EmojiPicker({
  groupId,
  onSelect,
  onClose,
  positionClass = 'bottom-full right-0 mb-2',
}: EmojiPickerProps) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const [query, setQuery] = useState('');
  const ids = useGroups((s) => s.ids);
  const states = useGroups((s) => s.states);
  // Contexte serveur : émojis du groupe courant. MP (`groupId` absent) :
  // agrégat dédupliqué de tous les serveurs rejoints (voir `aggregateEmojis`).
  const customs: AggregatedEmoji[] = useMemo(
    () =>
      groupId != null
        ? (states[groupId]?.emojis ?? []).map((e) => ({ ...e, groupId }))
        : aggregateEmojis(ids, states),
    [groupId, ids, states],
  );

  useEffect(() => {
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

  const q = query.trim().toLowerCase();
  const customsFiltres = customs.filter((e) => q === '' || e.name.includes(q));
  // MP : les customs viennent de plusieurs serveurs, le libellé le précise.
  const customSectionLabel = groupId != null ? t.emoji.customSection : t.emoji.customSectionDm;
  const categories = EMOJIS_UNICODE.map((cat) => ({
    id: cat.id,
    emojis: cat.emojis.filter((e) => correspond(e, q)),
  })).filter((cat) => cat.emojis.length > 0);

  const rien = customsFiltres.length === 0 && categories.length === 0;

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label={t.emoji.pickerLabel}
      className={`popover-enter absolute z-30 flex max-h-80 w-72 max-w-[90vw] flex-col rounded-lg border border-rail bg-modal shadow-elevation ${positionClass}`}
    >
      <div className="border-b border-rail p-2">
        <input
          type="text"
          autoFocus
          aria-label={t.emoji.search}
          placeholder={t.emoji.search}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="w-full rounded bg-rail px-2.5 py-1.5 text-sm text-norm placeholder-faint outline-none focus-visible:ring-2 focus-visible:ring-blurple"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {rien && (
          <p className="py-4 text-center text-sm text-muted">{t.emoji.noResult}</p>
        )}

        {customsFiltres.length > 0 && (
          <section aria-label={customSectionLabel} className="mb-2">
            <h4 className="mb-1 px-1 text-xs font-semibold uppercase tracking-wide text-faint">
              {customSectionLabel}
            </h4>
            <div className="flex flex-wrap gap-0.5">
              {customsFiltres.map((emoji) => (
                <button
                  key={emoji.name}
                  type="button"
                  aria-label={jetonEmojiTexte(emoji.name)}
                  title={jetonEmojiTexte(emoji.name)}
                  onClick={() =>
                    onSelect({
                      kind: 'custom',
                      name: emoji.name,
                      merkleRoot: emoji.merkle_root,
                    })
                  }
                  className="flex h-9 w-9 items-center justify-center rounded transition-colors duration-fast hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none active:scale-90"
                >
                  <CustomEmoji
                    name={emoji.name}
                    merkleRoot={emoji.merkle_root}
                    hint={emoji.groupId}
                    size={24}
                  />
                </button>
              ))}
            </div>
          </section>
        )}

        {categories.map((cat) => (
          <section key={cat.id} aria-label={labelCategorie(cat.id, t)} className="mb-2">
            <h4 className="mb-1 px-1 text-xs font-semibold uppercase tracking-wide text-faint">
              {labelCategorie(cat.id, t)}
            </h4>
            <div className="flex flex-wrap gap-0.5">
              {cat.emojis.map((emoji) => (
                <button
                  key={emoji.char}
                  type="button"
                  aria-label={interpolate(t.emoji.insert, { emoji: emoji.char })}
                  title={emoji.char}
                  onClick={() => onSelect({ kind: 'unicode', char: emoji.char })}
                  className="flex h-9 w-9 items-center justify-center rounded text-xl leading-none transition-transform duration-fast ease-spring hover:scale-110 hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none active:scale-90"
                >
                  {emoji.char}
                </button>
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
