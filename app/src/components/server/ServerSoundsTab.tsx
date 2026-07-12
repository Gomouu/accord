/**
 * Onglet Soundboard : ajout d'un clip audio (choix d'un fichier + nom validé
 * `[a-z0-9_]` 2-32), liste des sons existants avec préécoute locale (bouton ▶)
 * et suppression confirmée. Réservé à la permission `MANAGE_EMOJIS` (l'onglet
 * n'apparaît pas sinon, mais on revérifie ici). Contrairement aux émojis, aucun
 * ré-encodage : on vérifie seulement le type MIME et la taille (≤ 256 Kio),
 * puis on transmet le clip tel quel via `groups.sounds.add`.
 */

import { useRef, useState } from 'react';
import { interpolate } from '../../i18n';
import { fichierEnDataUrl } from '../../lib/attachments';
import { hasPerm, PERMISSIONS, useGroups } from '../../stores/groups';
import { playSound } from '../../stores/soundboard';
import { useUi, useT } from '../../stores/ui';
import { estMimeSonValide, estNomSonValide, estTailleSonValide } from '../../lib/sound';
import { SettingsSection } from '../settings/controls';
import { ConfirmButton, messageOf } from './controls';

/** Icône note de musique (emplacement d'ajout d'un son). */
function NoteIcon() {
  return (
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
      <path d="M9 18V5l12-2v13" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="18" cy="16" r="3" />
    </svg>
  );
}

/** Icône lecture (triangle) du bouton de préécoute. */
function PlayIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="currentColor"
      stroke="none"
      aria-hidden
    >
      <polygon points="6 4 20 12 6 20 6 4" />
    </svg>
  );
}

/** Nom de son proposé à partir d'un nom de fichier (épuré aux bornes). */
function nomDepuisFichier(nomFichier: string): string {
  const base = nomFichier.replace(/\.[^.]+$/, '').toLowerCase();
  return base
    .replace(/[^a-z0-9_]+/g, '_')
    .replace(/^_+|_+$/g, '')
    .slice(0, 32);
}

/** Clip choisi, décodé et prêt pour `groups.sounds.add`. */
interface SonChoisi {
  dataB64: string;
  mime: string;
  /** URL `data:` pour la préécoute avant l'envoi. */
  apercu: string;
}

export function ServerSoundsTab({ groupId }: { groupId: string }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const state = useGroups((s) => s.states[groupId]);
  const addSound = useGroups((s) => s.addSound);
  const delSound = useGroups((s) => s.delSound);
  const [name, setName] = useState('');
  const [son, setSon] = useState<SonChoisi | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);

  if (!state) return null;

  const canManage = hasPerm(state.my_permissions, PERMISSIONS.MANAGE_EMOJIS);
  const sounds = state.sounds ?? [];

  const choisirFichier = async (file: File): Promise<void> => {
    if (!estMimeSonValide(file.type)) {
      setErreur(t.soundboard.badFormat);
      return;
    }
    if (!estTailleSonValide(file.size)) {
      setErreur(t.soundboard.tooLarge);
      return;
    }
    try {
      const apercu = await fichierEnDataUrl(file);
      setSon({
        dataB64: apercu.slice(apercu.indexOf(',') + 1),
        mime: file.type,
        apercu,
      });
      setErreur(null);
      if (name === '') setName(nomDepuisFichier(file.name));
    } catch (e) {
      setErreur(messageOf(e, t.errors.actionFailed));
    }
  };

  const preecouterChoix = (): void => {
    if (son === null) return;
    new Audio(son.apercu).play().catch(() => {
      toast('error', t.soundboard.playbackFailed);
    });
  };

  const nomOk = estNomSonValide(name);
  const peutAjouter = canManage && nomOk && son !== null && !busy;

  const ajouter = async (): Promise<void> => {
    if (!peutAjouter || son === null) return;
    setBusy(true);
    try {
      await addSound(groupId, name, son.mime, son.dataB64);
      toast('info', t.soundboard.added);
      setName('');
      setSon(null);
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
        <SettingsSection title={t.soundboard.addTitle} hint={t.soundboard.hint}>
          <div className="rounded-lg bg-sidebar p-4">
            <div className="flex items-center gap-4">
              <button
                type="button"
                aria-label={t.soundboard.play}
                disabled={son === null}
                onClick={preecouterChoix}
                className="flex h-16 w-16 shrink-0 items-center justify-center rounded-lg bg-rail text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:cursor-default disabled:hover:text-faint"
              >
                {son !== null ? <PlayIcon /> : <NoteIcon />}
              </button>
              <div className="min-w-0 flex-1">
                <input
                  ref={fileRef}
                  type="file"
                  accept="audio/*"
                  aria-label={t.soundboard.chooseFile}
                  className="hidden"
                  onChange={(e) => {
                    const file = e.target.files?.[0];
                    e.target.value = '';
                    if (file !== undefined) void choisirFichier(file);
                  }}
                />
                <button
                  type="button"
                  disabled={busy}
                  onClick={() => fileRef.current?.click()}
                  className="rounded-lg bg-rail px-3 py-1.5 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
                >
                  {t.soundboard.chooseFile}
                </button>
                <div className="mt-2 flex items-center gap-1 text-sm text-faint">
                  <input
                    aria-label={t.soundboard.nameLabel}
                    placeholder={t.soundboard.namePlaceholder}
                    value={name}
                    maxLength={32}
                    onChange={(e) => setName(e.target.value.toLowerCase())}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') void ajouter();
                    }}
                    className="min-w-0 flex-1 rounded-md border border-transparent bg-input px-2 py-1 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
                  />
                </div>
              </div>
              <button
                type="button"
                disabled={!peutAjouter}
                onClick={() => void ajouter()}
                className="shrink-0 rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
              >
                {t.soundboard.add}
              </button>
            </div>
            {erreur !== null && (
              <p className="mt-2 text-sm text-red" role="alert">
                {erreur}
              </p>
            )}
          </div>
        </SettingsSection>
      )}

      <SettingsSection
        title={interpolate(t.soundboard.count, { count: String(sounds.length) })}
      >
        {sounds.length === 0 ? (
          <p className="text-sm text-muted">{t.soundboard.empty}</p>
        ) : (
          <div className="space-y-1">
            {sounds.map((sound) => (
              <div
                key={sound.name}
                className="flex items-center gap-3 rounded-lg bg-sidebar px-3 py-2"
              >
                <button
                  type="button"
                  aria-label={interpolate(t.soundboard.playOf, { name: sound.name })}
                  title={t.soundboard.play}
                  onClick={() => playSound(sound.merkle_root, groupId)}
                  className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-rail text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
                >
                  <PlayIcon />
                </button>
                <span className="min-w-0 flex-1 truncate font-mono text-sm text-norm">
                  {sound.name}
                </span>
                {canManage && (
                  <ConfirmButton
                    action={t.soundboard.delete}
                    question={interpolate(t.soundboard.deleteConfirm, {
                      name: sound.name,
                    })}
                    onConfirm={() => {
                      delSound(groupId, sound.name).catch((e: unknown) =>
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
