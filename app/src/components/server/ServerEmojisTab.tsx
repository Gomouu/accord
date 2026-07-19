/**
 * Onglet Émojis : ajout (choix d'une image + nom validé `[a-z0-9_]` 2-32),
 * grille des émojis existants (image + `:name:`) et suppression confirmée.
 * Réservé à la permission `MANAGE_EMOJIS` (l'onglet n'apparaît pas sinon, mais
 * on revérifie ici). L'image n'est pas recadrée ; elle est automatiquement
 * compressée côté client pour tenir sous la limite d'envoi (voir
 * `lib/compressEmojiImage.ts`) — seuls les GIF animés trop lourds sont rejetés,
 * un ré-encodage canvas en détruirait l'animation.
 */

import { useRef, useState } from 'react';
import { interpolate } from '../../i18n';
import { EmojiCompressionError, compressEmojiImage } from '../../lib/compressEmojiImage';
import {
  EMOJI_MAX_PAR_SERVEUR,
  EMOJI_MIMES,
  estMimeEmojiValide,
  estNomEmojiValide,
  jetonEmojiTexte,
} from '../../lib/emoji';
import { useGroups, hasPerm, PERMISSIONS } from '../../stores/groups';
import { useUi, useT } from '../../stores/ui';
import { SettingsSection } from '../settings/controls';
import { CustomEmoji } from '../CustomEmoji';
import { ConfirmButton, messageOf } from './controls';

/** Nom d'émoji proposé à partir d'un nom de fichier (épuré aux bornes). */
function nomDepuisFichier(nomFichier: string): string {
  const base = nomFichier.replace(/\.[^.]+$/, '').toLowerCase();
  return base
    .replace(/[^a-z0-9_]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 32);
}

/** Image choisie, décodée et prête pour `groups.emoji.add`. */
interface ImageChoisie {
  dataB64: string;
  mime: string;
  apercu: string;
}

export function ServerEmojisTab({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const addEmoji = useGroups((s) => s.addEmoji);
  const delEmoji = useGroups((s) => s.delEmoji);
  const [name, setName] = useState('');
  const [image, setImage] = useState<ImageChoisie | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  if (!state) return null;

  const canManage = hasPerm(state.my_permissions, PERMISSIONS.MANAGE_EMOJIS);
  const emojis = state.emojis ?? [];
  const plein = emojis.length >= EMOJI_MAX_PAR_SERVEUR;

  const choisirImage = async (file: File): Promise<void> => {
    if (!estMimeEmojiValide(file.type)) {
      setErreur(t.serveur.emojiInvalidImage);
      return;
    }
    try {
      const compresse = await compressEmojiImage(file);
      setImage({
        dataB64: compresse.dataB64,
        mime: compresse.mime,
        apercu: compresse.dataUrl,
      });
      setErreur(null);
      if (name === '') setName(nomDepuisFichier(file.name));
    } catch (e) {
      const animeTropLourd =
        e instanceof EmojiCompressionError && e.raison === 'anime-trop-lourd';
      setErreur(
        animeTropLourd ? t.serveur.emojiAnimatedTooLarge : t.serveur.emojiInvalidImage,
      );
    }
  };

  const nomOk = estNomEmojiValide(name);
  const peutAjouter = canManage && !plein && nomOk && image !== null && !busy;

  const ajouter = async (): Promise<void> => {
    if (!peutAjouter || image === null) return;
    setBusy(true);
    try {
      await addEmoji(groupId, name, image.dataB64, image.mime);
      toast('info', t.serveur.emojiAdded);
      setName('');
      setImage(null);
      setErreur(null);
    } catch (e) {
      setErreur(messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      {canManage && (
        <SettingsSection title={t.serveur.emojiTitle} hint={t.serveur.emojiHint}>
          <div className="rounded-lg bg-sidebar p-4">
            <div className="flex items-center gap-4">
              <div className="flex h-16 w-16 shrink-0 items-center justify-center overflow-hidden rounded-lg bg-rail text-faint">
                {image !== null ? (
                  <img
                    src={image.apercu}
                    alt=""
                    width={64}
                    height={64}
                    className="h-full w-full object-contain"
                  />
                ) : (
                  <svg
                    width="28"
                    height="28"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth={2}
                    strokeLinecap="round"
                    strokeLinejoin="round"
                    aria-hidden
                  >
                    <circle cx="12" cy="12" r="10" />
                    <path d="M8 14s1.5 2 4 2 4-2 4-2" />
                    <line x1="9" x2="9.01" y1="9" y2="9" />
                    <line x1="15" x2="15.01" y1="9" y2="9" />
                  </svg>
                )}
              </div>
              <div className="min-w-0 flex-1">
                <input
                  ref={fileRef}
                  type="file"
                  accept={EMOJI_MIMES.join(',')}
                  aria-label={t.serveur.emojiChooseImage}
                  className="hidden"
                  onChange={(e) => {
                    const file = e.target.files?.[0];
                    e.target.value = '';
                    if (file !== undefined) void choisirImage(file);
                  }}
                />
                <button
                  type="button"
                  disabled={plein || busy}
                  onClick={() => fileRef.current?.click()}
                  className="rounded-lg bg-rail px-3 py-1.5 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
                >
                  {t.serveur.emojiChooseImage}
                </button>
                <div className="mt-2 flex items-center gap-1 text-sm text-faint">
                  <span aria-hidden>:</span>
                  <input
                    aria-label={t.serveur.emojiNameLabel}
                    placeholder={t.serveur.emojiNamePlaceholder}
                    value={name}
                    maxLength={32}
                    onChange={(e) => setName(e.target.value.toLowerCase())}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') void ajouter();
                    }}
                    className="min-w-0 flex-1 rounded-md border border-transparent bg-input px-2 py-1 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
                  />
                  <span aria-hidden>:</span>
                </div>
              </div>
              <button
                type="button"
                disabled={!peutAjouter}
                onClick={() => void ajouter()}
                className="shrink-0 rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
              >
                {t.serveur.emojiAdd}
              </button>
            </div>
            {erreur !== null && (
              <p className="mt-2 text-sm text-red" role="alert">
                {erreur}
              </p>
            )}
            {plein && <p className="mt-2 text-xs text-faint">{t.serveur.emojiLimit}</p>}
          </div>
        </SettingsSection>
      )}

      <SettingsSection
        title={interpolate(t.serveur.emojiCount, { count: String(emojis.length) })}
      >
        {emojis.length === 0 ? (
          <p className="text-sm text-muted">{t.serveur.emojiEmpty}</p>
        ) : (
          <div className="space-y-1">
            {emojis.map((emoji) => (
              <div
                key={emoji.name}
                className="flex items-center gap-3 rounded-lg bg-sidebar px-3 py-2"
              >
                <div className="flex h-9 w-9 shrink-0 items-center justify-center">
                  <CustomEmoji
                    name={emoji.name}
                    merkleRoot={emoji.merkle_root}
                    hint={groupId}
                    size={32}
                  />
                </div>
                <span className="min-w-0 flex-1 truncate font-mono text-sm text-norm">
                  {jetonEmojiTexte(emoji.name)}
                </span>
                {canManage && (
                  <ConfirmButton
                    action={t.serveur.emojiDelete}
                    question={interpolate(t.serveur.emojiDeleteConfirm, {
                      name: jetonEmojiTexte(emoji.name),
                    })}
                    onConfirm={() => {
                      delEmoji(groupId, emoji.name).catch((e: unknown) =>
                        toast('error', messageOf(e, t.errors.actionFailed)),
                      );
                    }}
                  />
                )}
              </div>
            ))}
          </div>
        )}
      </SettingsSection>
    </div>
  );
}
