/**
 * Onglet Stickers : ajout (choix d'une image + nom validé `[a-z0-9_]` 2-32),
 * grille des stickers existants (image + `:name:`) et suppression confirmée.
 * Réservé à la permission `MANAGE_EMOJIS` (même famille que les émojis de
 * serveur — l'onglet n'apparaît pas sinon, mais on revérifie ici). Mêmes
 * conventions que `ServerEmojisTab`, en réutilisant le même pipeline de
 * compression (`compressEmojiImage`) paramétré pour la limite et le gabarit
 * d'un sticker (512 Kio, ~320 px) plutôt que de le dupliquer.
 */

import { useRef, useState } from 'react';
import { interpolate } from '../../i18n';
import { EmojiCompressionError, compressEmojiImage } from '../../lib/compressEmojiImage';
import { jetonEmojiTexte } from '../../lib/emoji';
import {
  estMimeStickerValide,
  estNomStickerValide,
  STICKER_MAX_PAR_SERVEUR,
  STICKER_MIMES,
  STICKER_OCTETS_MAX,
  STICKER_PALIERS_TAILLE,
} from '../../lib/sticker';
import { useGroups, hasPerm, PERMISSIONS } from '../../stores/groups';
import { useUi, useT } from '../../stores/ui';
import { SettingsSection } from '../settings/controls';
import { StickerImage } from '../StickerImage';
import { ConfirmButton, messageOf } from './controls';

/** Nom de sticker proposé à partir d'un nom de fichier (épuré aux bornes). */
function nomDepuisFichier(nomFichier: string): string {
  const base = nomFichier.replace(/\.[^.]+$/, '').toLowerCase();
  return base
    .replace(/[^a-z0-9_]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 32);
}

/** Image choisie, décodée et prête pour `groups.stickers.add`. */
interface ImageChoisie {
  dataB64: string;
  mime: string;
  apercu: string;
}

export function ServerStickersTab({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const addSticker = useGroups((s) => s.addSticker);
  const removeSticker = useGroups((s) => s.removeSticker);
  const [name, setName] = useState('');
  const [image, setImage] = useState<ImageChoisie | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  if (!state) return null;

  const canManage = hasPerm(state.my_permissions, PERMISSIONS.MANAGE_EMOJIS);
  const stickers = state.stickers ?? [];
  const plein = stickers.length >= STICKER_MAX_PAR_SERVEUR;

  const choisirImage = async (file: File): Promise<void> => {
    if (!estMimeStickerValide(file.type)) {
      setErreur(t.serveur.stickerInvalidImage);
      return;
    }
    try {
      const compresse = await compressEmojiImage(file, {
        maxBytes: STICKER_OCTETS_MAX,
        sizes: STICKER_PALIERS_TAILLE,
      });
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
        animeTropLourd
          ? t.serveur.stickerAnimatedTooLarge
          : t.serveur.stickerInvalidImage,
      );
    }
  };

  const nomOk = estNomStickerValide(name);
  const peutAjouter = canManage && !plein && nomOk && image !== null && !busy;

  const ajouter = async (): Promise<void> => {
    if (!peutAjouter || image === null) return;
    setBusy(true);
    try {
      await addSticker(groupId, name, image.dataB64, image.mime);
      toast('info', t.serveur.stickerAdded);
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
        <SettingsSection title={t.serveur.stickerTitle} hint={t.serveur.stickerHint}>
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
                    <path d="M15.5 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V8.5L15.5 3Z" />
                    <path d="M15 3v6h6" />
                  </svg>
                )}
              </div>
              <div className="min-w-0 flex-1">
                <input
                  ref={fileRef}
                  type="file"
                  accept={STICKER_MIMES.join(',')}
                  aria-label={t.serveur.stickerChooseImage}
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
                  {t.serveur.stickerChooseImage}
                </button>
                <div className="mt-2 flex items-center gap-1 text-sm text-faint">
                  <span aria-hidden>:</span>
                  <input
                    aria-label={t.serveur.stickerNameLabel}
                    placeholder={t.serveur.stickerNamePlaceholder}
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
                {t.serveur.stickerAdd}
              </button>
            </div>
            {erreur !== null && (
              <p className="mt-2 text-sm text-red" role="alert">
                {erreur}
              </p>
            )}
            {plein && <p className="mt-2 text-xs text-faint">{t.serveur.stickerLimit}</p>}
          </div>
        </SettingsSection>
      )}

      <SettingsSection
        title={interpolate(t.serveur.stickerCount, { count: String(stickers.length) })}
      >
        {stickers.length === 0 ? (
          <p className="text-sm text-muted">{t.serveur.stickerEmpty}</p>
        ) : (
          <div className="space-y-1">
            {stickers.map((sticker) => (
              <div
                key={sticker.name}
                className="flex items-center gap-3 rounded-lg bg-sidebar px-3 py-2"
              >
                <div className="flex h-10 w-10 shrink-0 items-center justify-center">
                  <StickerImage
                    name={sticker.name}
                    merkleRoot={sticker.merkle_root}
                    hint={groupId}
                    size={40}
                  />
                </div>
                <span className="min-w-0 flex-1 truncate font-mono text-sm text-norm">
                  {jetonEmojiTexte(sticker.name)}
                </span>
                {canManage && (
                  <ConfirmButton
                    action={t.serveur.stickerDelete}
                    question={interpolate(t.serveur.stickerDeleteConfirm, {
                      name: jetonEmojiTexte(sticker.name),
                    })}
                    onConfirm={() => {
                      removeSticker(groupId, sticker.name).catch((e: unknown) =>
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
