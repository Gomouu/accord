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
import { useEmojiRecents } from '../stores/recents';
import { bouclerTab } from '../lib/focus';
import { interpolate } from '../i18n';
import type { Dict } from '../i18n';
import { useT } from '../stores/ui';
import { CustomEmoji } from './CustomEmoji';
import { StickerImage } from './StickerImage';

interface EmojiPickerProps {
  /** Contexte serveur : expose ses émojis custom (`null`/absent = MP). */
  groupId?: string | null;
  onSelect: (pick: EmojiPick) => void;
  onClose: () => void;
  /** Classes de placement du panneau (positionné par l'appelant). */
  positionClass?: string;
  /**
   * Stickers du serveur courant : présent uniquement en contexte de groupe
   * (`groupId` non nul) où l'appelant sait envoyer un sticker. Choisir un
   * sticker envoie immédiatement le message (jamais inséré dans le composeur)
   * — absent = pas de section Stickers (MP, ou barre d'actions au survol
   * d'un message, où l'envoi d'un nouveau message n'a pas de sens).
   */
  onPickSticker?: (name: string, merkleRoot: string) => void;
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

/** Vrai si un émoji récent correspond à la recherche (caractère ou nom custom). */
function correspondRecent(pick: EmojiPick, q: string): boolean {
  if (q === '') return true;
  return pick.kind === 'unicode' ? pick.char.includes(q) : pick.name.includes(q);
}

export function EmojiPicker({
  groupId,
  onSelect,
  onClose,
  positionClass = 'bottom-full end-0 mb-2',
  onPickSticker,
}: EmojiPickerProps) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const rechercheRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState('');
  // Émojis récents (Unicode + custom) : store partagé avec la barre de
  // réactions rapides (voir `stores/recents`), persisté en localStorage.
  const recents = useEmojiRecents((s) => s.list);
  const addRecentPick = useEmojiRecents((s) => s.add);
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
  // Stickers : uniquement en contexte de groupe (jamais agrégés en MP — un
  // sticker ne peut être envoyé que dans le salon d'un serveur).
  const stickers = groupId != null ? (states[groupId]?.stickers ?? []) : [];

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

  // Le champ de recherche prend le focus à l'ouverture (après capture du
  // déclencheur — `autoFocus` s'appliquerait avant, faussant la capture), et
  // le déclencheur le récupère à la fermeture.
  useEffect(() => {
    const declencheur =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    rechercheRef.current?.focus();
    return () => {
      if (declencheur !== null && declencheur.isConnected) declencheur.focus();
    };
  }, []);

  // Enregistre le choix en tête des récents (store partagé) puis délègue.
  const handleSelect = (pick: EmojiPick): void => {
    addRecentPick(pick);
    onSelect(pick);
  };

  const q = query.trim().toLowerCase();
  const recentsFiltres = recents.filter((r) => correspondRecent(r, q));
  const customsFiltres = customs.filter((e) => q === '' || e.name.includes(q));
  const stickersFiltres = stickers.filter((s) => q === '' || s.name.includes(q));
  // MP : les customs viennent de plusieurs serveurs, le libellé le précise.
  const customSectionLabel =
    groupId != null ? t.emoji.customSection : t.emoji.customSectionDm;
  const categories = EMOJIS_UNICODE.map((cat) => ({
    id: cat.id,
    emojis: cat.emojis.filter((e) => correspond(e, q)),
  })).filter((cat) => cat.emojis.length > 0);

  const rien =
    recentsFiltres.length === 0 &&
    customsFiltres.length === 0 &&
    stickersFiltres.length === 0 &&
    categories.length === 0;

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label={t.emoji.pickerLabel}
      onKeyDown={(e) => bouclerTab(e, ref.current)}
      className={`glass-strong popover-enter absolute z-30 flex max-h-80 w-72 max-w-[90vw] flex-col rounded-lg ${positionClass}`}
    >
      <div className="border-b border-input/50 p-2">
        <input
          ref={rechercheRef}
          type="text"
          aria-label={t.emoji.search}
          placeholder={t.emoji.search}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          className="w-full rounded-xl border border-transparent bg-input px-2.5 py-1.5 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
        />
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto p-2">
        {rien && (
          <p className="py-4 text-center text-sm text-muted">{t.emoji.noResult}</p>
        )}

        {recentsFiltres.length > 0 && (
          <section aria-label={t.emoji.recents} className="mb-2">
            <h4 className="mb-1 px-1 text-xs font-medium uppercase tracking-wide text-faint">
              {t.emoji.recents}
            </h4>
            <div className="flex flex-wrap gap-0.5">
              {recentsFiltres.map((pick) =>
                pick.kind === 'unicode' ? (
                  <button
                    key={`u:${pick.char}`}
                    type="button"
                    aria-label={interpolate(t.emoji.insert, { emoji: pick.char })}
                    title={pick.char}
                    onClick={() => handleSelect(pick)}
                    className="flex h-9 w-9 items-center justify-center rounded-md text-xl leading-none transition-transform duration-fast ease-spring hover:scale-110 hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none active:scale-90"
                  >
                    {pick.char}
                  </button>
                ) : (
                  <button
                    key={`c:${pick.name}`}
                    type="button"
                    aria-label={jetonEmojiTexte(pick.name)}
                    title={jetonEmojiTexte(pick.name)}
                    onClick={() => handleSelect(pick)}
                    className="flex h-9 w-9 items-center justify-center rounded-md transition-transform duration-fast ease-spring hover:scale-110 hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none active:scale-90"
                  >
                    <CustomEmoji
                      name={pick.name}
                      merkleRoot={pick.merkleRoot}
                      size={24}
                    />
                  </button>
                ),
              )}
            </div>
          </section>
        )}

        {customsFiltres.length > 0 && (
          <section aria-label={customSectionLabel} className="mb-2">
            <h4 className="mb-1 px-1 text-xs font-medium uppercase tracking-wide text-faint">
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
                    handleSelect({
                      kind: 'custom',
                      name: emoji.name,
                      merkleRoot: emoji.merkle_root,
                    })
                  }
                  className="flex h-9 w-9 items-center justify-center rounded-md transition-transform duration-fast ease-spring hover:scale-110 hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none active:scale-90"
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

        {stickersFiltres.length > 0 && onPickSticker !== undefined && (
          <section aria-label={t.emoji.stickersSection} className="mb-2">
            <h4 className="mb-1 px-1 text-xs font-medium uppercase tracking-wide text-faint">
              {t.emoji.stickersSection}
            </h4>
            <div className="flex flex-wrap gap-1">
              {stickersFiltres.map((sticker) => (
                <button
                  key={sticker.name}
                  type="button"
                  aria-label={jetonEmojiTexte(sticker.name)}
                  title={jetonEmojiTexte(sticker.name)}
                  onClick={() => onPickSticker(sticker.name, sticker.merkle_root)}
                  className="flex h-14 w-14 items-center justify-center rounded-md transition-transform duration-fast ease-spring hover:scale-105 hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none active:scale-95"
                >
                  <StickerImage
                    name={sticker.name}
                    merkleRoot={sticker.merkle_root}
                    hint={groupId ?? undefined}
                    size={48}
                  />
                </button>
              ))}
            </div>
          </section>
        )}

        {categories.map((cat) => (
          <section key={cat.id} aria-label={labelCategorie(cat.id, t)} className="mb-2">
            <h4 className="mb-1 px-1 text-xs font-medium uppercase tracking-wide text-faint">
              {labelCategorie(cat.id, t)}
            </h4>
            <div className="flex flex-wrap gap-0.5">
              {cat.emojis.map((emoji) => (
                <button
                  key={emoji.char}
                  type="button"
                  aria-label={interpolate(t.emoji.insert, { emoji: emoji.char })}
                  title={emoji.char}
                  onClick={() => handleSelect({ kind: 'unicode', char: emoji.char })}
                  className="flex h-9 w-9 items-center justify-center rounded-md text-xl leading-none transition-transform duration-fast ease-spring hover:scale-110 hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none active:scale-90"
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
