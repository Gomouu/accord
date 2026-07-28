/**
 * Onglet AutoMod : liste des mots filtrés du serveur (chips retirables) et
 * champ d'ajout (Entrée), enregistrée d'un bloc via `groups.automod.set` —
 * gouverné par MANAGE_CHANNELS (visibilité gérée par `ServerSettingsModal`).
 * Modèle serverless : les clients honnêtes masquent ces mots au rendu, rien
 * n'est supprimé du réseau.
 *
 * 🔒 Les bornes affichées viennent de `lib/automod.ts`, qui reprend celles du
 * NŒUD. Elles vivaient auparavant en dur ici, à 100 mots, alors que le format
 * filaire en refuse plus de 50 : passé le cinquantième, l'écran laissait
 * ajouter puis « Enregistrer » échouait sur une erreur venue du nœud, sans que
 * rien n'ait prévenu.
 */

import { useState } from 'react';
import { interpolate } from '../../i18n';
import { MAX_AUTOMOD_WORDS, MAX_AUTOMOD_WORD_CHARS } from '../../lib/automod';
import { useGroups } from '../../stores/groups';
import { useUi, useT } from '../../stores/ui';
import { CloseIcon } from '../ContextMenu';
import { SettingsSection } from '../settings/controls';
import { messageOf } from './controls';

export function ServerAutomodTab({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const stateWords = useGroups((s) => s.states[groupId]?.automod_words);
  const setAutomodWords = useGroups((s) => s.setAutomodWords);
  /** Liste éditée localement, envoyée d'un bloc au bouton Enregistrer. */
  const [words, setWords] = useState<string[]>(() => [...(stateWords ?? [])]);
  const [draft, setDraft] = useState('');
  const [erreur, setErreur] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const ajouterMot = (): void => {
    const mot = draft.trim().toLowerCase();
    if (mot === '') return;
    // Longueur en CARACTÈRES (pas en unités de code) : le nœud compte des
    // caractères Unicode, et `mot.length` refusait à tort un mot d'emoji ou
    // d'idéogrammes bien en deçà de la borne réelle.
    if ([...mot].length > MAX_AUTOMOD_WORD_CHARS) {
      setErreur(
        interpolate(t.automod.wordTooLong, { max: String(MAX_AUTOMOD_WORD_CHARS) }),
      );
      return;
    }
    if (words.includes(mot)) {
      setErreur(t.automod.duplicate);
      return;
    }
    if (words.length >= MAX_AUTOMOD_WORDS) {
      setErreur(interpolate(t.automod.limitReached, { max: String(MAX_AUTOMOD_WORDS) }));
      return;
    }
    setWords((prev) => [...prev, mot]);
    setDraft('');
    setErreur(null);
  };

  const retirerMot = (mot: string): void => {
    setWords((prev) => prev.filter((w) => w !== mot));
    setErreur(null);
  };

  // Modifiée par rapport à l'état du nœud (comparaison insensible à l'ordre :
  // l'état matérialisé peut renvoyer la liste dans un ordre différent).
  const reference = stateWords ?? [];
  const dirty =
    words.length !== reference.length ||
    words.some((w) => !reference.includes(w)) ||
    reference.some((w) => !words.includes(w));

  const save = async (): Promise<void> => {
    if (busy || !dirty) return;
    setBusy(true);
    try {
      // L'événement d'état (`event.group_state`) rafraîchit la liste affichée.
      await setAutomodWords(groupId, words);
      toast('success', t.automod.saved);
    } catch (e) {
      toast('error', messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <p className="mb-6 text-sm text-muted">
        {interpolate(t.automod.hint, {
          max: String(MAX_AUTOMOD_WORDS),
          chars: String(MAX_AUTOMOD_WORD_CHARS),
        })}
      </p>

      <SettingsSection
        title={interpolate(t.automod.count, {
          count: String(words.length),
          max: String(MAX_AUTOMOD_WORDS),
        })}
      >
        <div className="rounded-lg bg-sidebar p-3">
          {words.length === 0 ? (
            <p className="text-sm text-muted">{t.automod.empty}</p>
          ) : (
            <div className="flex flex-wrap gap-2">
              {words.map((mot) => (
                <span
                  key={mot}
                  className="flex items-center gap-1.5 rounded-full bg-rail px-3 py-1 text-sm text-norm"
                >
                  {mot}
                  <button
                    type="button"
                    aria-label={interpolate(t.automod.removeWord, { word: mot })}
                    title={interpolate(t.automod.removeWord, { word: mot })}
                    disabled={busy}
                    onClick={() => retirerMot(mot)}
                    // 16 px de croix dans une pastille de 28 px : grossir la
                    // boîte déformerait la pastille elle-même. `hit-area`
                    // étend la seule zone cliquable, sur toute la hauteur de
                    // la pastille et jusqu'à l'écart qui la sépare de la
                    // suivante.
                    className="hit-area relative rounded-full p-0.5 text-faint transition-colors duration-fast hover:text-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-40 [--hit-block:6px] [--hit-inline:14px]"
                  >
                    <CloseIcon size={12} />
                  </button>
                </span>
              ))}
            </div>
          )}

          <div className="mt-3 flex gap-3">
            <input
              aria-label={t.automod.addPlaceholder}
              placeholder={t.automod.addPlaceholder}
              value={draft}
              onChange={(e) => {
                setDraft(e.target.value);
                setErreur(null);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  ajouterMot();
                }
              }}
              className="min-w-0 flex-1 rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
            />
            <button
              type="button"
              disabled={draft.trim() === '' || busy}
              onClick={ajouterMot}
              className="rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
            >
              {t.automod.addAction}
            </button>
          </div>
          {erreur !== null && (
            <p className="mt-2 text-sm text-red" role="alert">
              {erreur}
            </p>
          )}
        </div>

        <div className="mt-3 flex justify-end">
          <button
            type="button"
            disabled={busy || !dirty}
            onClick={() => void save()}
            className="rounded-lg bg-blurple px-4 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat disabled:opacity-50"
          >
            {t.automod.save}
          </button>
        </div>
      </SettingsSection>
    </div>
  );
}
