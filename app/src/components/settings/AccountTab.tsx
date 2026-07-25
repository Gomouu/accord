/**
 * Onglet Mon compte : avatar (image recadrée en carré, réduite à 256 px,
 * publiée via profile.set_avatar), bannière, pseudo, pronoms et bio
 * (profile.set), personnalisation du profil (décoration, effet, cadre),
 * code ami copiable, clé publique abrégée et rappel sur la phrase de
 * récupération (affichée une seule fois à la création, non ré-affichable).
 */

import { useEffect, useRef, useState } from 'react';
import {
  isValidName,
  BIO_MAX,
  NAME_MAX,
  PRONOUNS_MAX,
  useSession,
} from '../../stores/session';
import { useUi, useSettingsT, useT } from '../../stores/ui';
import { backupExport, backupImport } from '../../lib/bridge';
import { lireFichier } from '../../lib/files';
import { AvatarCropper } from '../AvatarCropper';
import { Avatar } from '../Avatar';
import { SettingsSection } from './controls';
import { DevicesSection } from './DevicesSection';
import { ProfilePersonalization } from './ProfilePersonalization';

const COPY_FEEDBACK_MS = 1500;

/** Clé publique abrégée : assez pour comparer, sans mur d'hexadécimal. */
function abbreviate(pubkey: string): string {
  if (pubkey.length <= 20) return pubkey;
  return `${pubkey.slice(0, 12)}…${pubkey.slice(-8)}`;
}

/** Section avatar : aperçu, choix d'image (recadrage + 256 px), retrait. */
function AvatarSection() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const self = useSession((s) => s.self);
  const setAvatar = useSession((s) => s.setAvatar);
  const [busy, setBusy] = useState(false);
  /** Aperçu local de l'image fraîchement envoyée (avant relecture du hash). */
  const [preview, setPreview] = useState<string | null>(null);
  /** Image en cours de recadrage (recadreur ouvert tant que non nulle). */
  const [cropFile, setCropFile] = useState<File | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  if (!self) return null;

  const onCrop = async (
    dataB64: string,
    mime: string,
    dataUrl: string,
  ): Promise<void> => {
    setBusy(true);
    try {
      await setAvatar(dataB64, mime);
      setPreview(dataUrl);
      toast('info', ts.settings.avatarUpdated);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
      setCropFile(null);
    }
  };

  const remove = async (): Promise<void> => {
    setBusy(true);
    try {
      await setAvatar(null);
      setPreview(null);
      toast('info', ts.settings.avatarRemoved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsSection title={ts.settings.avatarTitle} hint={ts.settings.avatarHint}>
      <div className="flex items-center gap-4 rounded-lg bg-sidebar p-4">
        <Avatar
          id={self.pubkey}
          name={self.name ?? self.friend_code}
          size={80}
          avatarHash={self.avatar}
          imageUrl={preview}
          hint={self.pubkey}
          decoration={self.avatar_decoration}
        />
        <input
          ref={fileRef}
          type="file"
          accept="image/*"
          aria-label={ts.settings.avatarChoose}
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            // Autorise de re-choisir le même fichier plus tard.
            e.target.value = '';
            if (file !== undefined) setCropFile(file);
          }}
        />
        <button
          type="button"
          disabled={busy}
          onClick={() => fileRef.current?.click()}
          className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
        >
          {ts.settings.avatarChoose}
        </button>
        {(self.avatar !== null || preview !== null) && (
          <button
            type="button"
            disabled={busy}
            onClick={() => void remove()}
            className="rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
          >
            {ts.settings.avatarRemove}
          </button>
        )}
      </div>
      {cropFile !== null && (
        <AvatarCropper
          fichier={cropFile}
          forme="cercle"
          onAnnuler={() => setCropFile(null)}
          onValider={(r) => onCrop(r.dataB64, r.mime, r.dataUrl)}
        />
      )}
    </SettingsSection>
  );
}

/**
 * Aperçu paysage de la bannière : priorité à l'aperçu local fraîchement
 * envoyé, sinon lecture du blob par son hash Merkle, sinon fond neutre.
 */
function BannerPreview({
  preview,
  hash,
  hint,
  label,
}: {
  preview: string | null;
  hash: string | null;
  hint: string;
  label: string;
}) {
  const [url, setUrl] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setUrl(null);
    if (hash === null) return undefined;
    lireFichier(hash, hint)
      .then((blobUrl) => {
        if (alive) setUrl(blobUrl);
      })
      .catch(() => {
        // Bannière indisponible : on reste sur le fond neutre.
      });
    return () => {
      alive = false;
    };
  }, [hash, hint]);

  const src = preview ?? url;
  if (src === null) {
    return <div className="h-24 w-full rounded-lg bg-rail" aria-hidden />;
  }
  return <img src={src} alt={label} className="h-24 w-full rounded-lg object-cover" />;
}

/** Section bannière : aperçu paysage, choix d'image (recadrage 3:1), retrait. */
function BannerSection() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const self = useSession((s) => s.self);
  const setBanner = useSession((s) => s.setBanner);
  const [busy, setBusy] = useState(false);
  /** Aperçu local de la bannière fraîchement envoyée (avant relecture du hash). */
  const [preview, setPreview] = useState<string | null>(null);
  /** Image en cours de recadrage (recadreur ouvert tant que non nulle). */
  const [cropFile, setCropFile] = useState<File | null>(null);
  const fileRef = useRef<HTMLInputElement>(null);

  if (!self) return null;

  const onCrop = async (
    dataB64: string,
    mime: string,
    dataUrl: string,
  ): Promise<void> => {
    setBusy(true);
    try {
      await setBanner(dataB64, mime);
      setPreview(dataUrl);
      toast('info', ts.settings.bannerUpdated);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
      setCropFile(null);
    }
  };

  const remove = async (): Promise<void> => {
    setBusy(true);
    try {
      await setBanner(null);
      setPreview(null);
      toast('info', ts.settings.bannerRemoved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsSection title={ts.settings.bannerTitle} hint={ts.settings.bannerHint}>
      <div className="rounded-lg bg-sidebar p-4">
        <BannerPreview
          preview={preview}
          hash={self.banner}
          hint={self.pubkey}
          label={ts.settings.bannerTitle}
        />
        <input
          ref={fileRef}
          type="file"
          accept="image/*"
          aria-label={ts.settings.bannerChoose}
          className="hidden"
          onChange={(e) => {
            const file = e.target.files?.[0];
            // Autorise de re-choisir le même fichier plus tard.
            e.target.value = '';
            if (file !== undefined) setCropFile(file);
          }}
        />
        <div className="mt-4 flex gap-3">
          <button
            type="button"
            disabled={busy}
            onClick={() => fileRef.current?.click()}
            className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
          >
            {ts.settings.bannerChoose}
          </button>
          {(self.banner !== null || preview !== null) && (
            <button
              type="button"
              disabled={busy}
              onClick={() => void remove()}
              className="rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
            >
              {ts.settings.bannerRemove}
            </button>
          )}
        </div>
      </div>
      {cropFile !== null && (
        <AvatarCropper
          fichier={cropFile}
          forme="banniere"
          onAnnuler={() => setCropFile(null)}
          onValider={(r) => onCrop(r.dataB64, r.mime, r.dataUrl)}
        />
      )}
    </SettingsSection>
  );
}

/** Section pronoms : champ court, chaîne vide = effacer. */
function PronounsSection() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const self = useSession((s) => s.self);
  const setPronouns = useSession((s) => s.setPronouns);
  const [draft, setDraft] = useState(self?.pronouns ?? '');
  const [busy, setBusy] = useState(false);

  if (!self) return null;

  const trimmed = draft.trim();
  const dirty = trimmed !== (self.pronouns ?? '');

  const save = async (): Promise<void> => {
    if (!dirty || busy || trimmed.length > PRONOUNS_MAX) return;
    setBusy(true);
    try {
      await setPronouns(trimmed);
      toast('success', ts.settings.pronounsSaved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsSection title={ts.settings.pronounsTitle} hint={ts.settings.pronounsHint}>
      <div className="flex gap-3 rounded-lg bg-sidebar p-3">
        <input
          aria-label={ts.settings.pronounsTitle}
          placeholder={ts.settings.pronounsPlaceholder}
          value={draft}
          maxLength={PRONOUNS_MAX}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void save();
          }}
          className="min-w-0 flex-1 rounded-md border border-transparent bg-input px-3 py-2 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
        />
        <button
          type="button"
          disabled={!dirty || busy}
          onClick={() => void save()}
          className="shrink-0 rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
        >
          {ts.settings.pronounsSave}
        </button>
      </div>
    </SettingsSection>
  );
}

/** Section bio : zone de texte avec compteur, chaîne vide = effacer. */
function BioSection() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const self = useSession((s) => s.self);
  const setBio = useSession((s) => s.setBio);
  const [draft, setDraft] = useState(self?.bio ?? '');
  const [busy, setBusy] = useState(false);

  if (!self) return null;

  const trimmed = draft.trim();
  const dirty = trimmed !== (self.bio ?? '');

  const save = async (): Promise<void> => {
    if (!dirty || busy || trimmed.length > BIO_MAX) return;
    setBusy(true);
    try {
      await setBio(trimmed);
      toast('success', ts.settings.bioSaved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsSection title={ts.settings.bioTitle} hint={ts.settings.bioHint}>
      <div className="rounded-lg bg-sidebar p-3">
        <textarea
          aria-label={ts.settings.bioTitle}
          placeholder={ts.settings.bioPlaceholder}
          value={draft}
          rows={3}
          maxLength={BIO_MAX}
          onChange={(e) => setDraft(e.target.value)}
          className="w-full resize-none rounded-md border border-transparent bg-input px-3 py-2 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
        />
        <div className="mt-2 flex items-center justify-between">
          <span className="text-xs text-faint">
            {draft.length}/{BIO_MAX}
          </span>
          <button
            type="button"
            disabled={!dirty || busy}
            onClick={() => void save()}
            className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
          >
            {ts.settings.bioSave}
          </button>
        </div>
      </div>
    </SettingsSection>
  );
}

/**
 * Section Sauvegarde : export du profil complet dans une archive
 * `.accordbackup` (fichiers chiffrés copiés tels quels — coffre scellé, base
 * SQLCipher, blobs) et import d'une archive comme compte NEUF, jamais sur le
 * profil actif.
 *
 * L'export arrête le nœud pour copier la base FERMÉE (invariant de
 * `accord_node::backup`) : la session se verrouille. L'avertissement est
 * affiché en permanence et le sélecteur natif « Enregistrer sous » sert de
 * confirmation — l'annuler n'invoque jamais la commande, rien ne se
 * verrouille. Après un export réussi, la modale est fermée puis `lock()`
 * aligne l'UI sur l'écran de déverrouillage (idempotent côté hôte : le nœud
 * est déjà arrêté). En cas d'échec après l'arrêt du nœud, aucune archive
 * tronquée n'existe (écriture atomique) et le bandeau hors-ligne guide vers
 * une nouvelle tentative ou une déconnexion manuelle.
 */
/** Détecte la « mauvaise phrase de passe » dans le message d'erreur de l'hôte. */
function estMauvaisePhrase(erreur: unknown): boolean {
  const message = erreur instanceof Error ? erreur.message : String(erreur);
  return message.includes('secret de déverrouillage incorrect');
}

function BackupSection() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const closeModal = useUi((s) => s.closeModal);
  const lock = useSession((s) => s.lock);
  const [busy, setBusy] = useState(false);
  // `null` = aucune saisie ouverte ; sinon on attend la phrase de passe pour
  // l'action choisie (l'export exige une phrase, l'import l'accepte vide pour
  // les anciennes sauvegardes non chiffrées).
  const [mode, setMode] = useState<'export' | 'import' | null>(null);
  const [phrase, setPhrase] = useState('');

  const fermerSaisie = (): void => {
    setMode(null);
    setPhrase('');
  };

  const doExport = async (): Promise<void> => {
    if (phrase === '') {
      toast('error', ts.settings.backupPassphraseEmpty);
      return;
    }
    setBusy(true);
    try {
      const statut = await backupExport(phrase);
      // Sélecteur annulé : la commande n'a pas été invoquée, rien ne change.
      if (statut === null) return;
      fermerSaisie();
      toast('info', ts.settings.backupExportDone);
      // Le nœud est déjà arrêté côté hôte : ferme la modale (l'écran de
      // déverrouillage ne doit jamais rester sous une modale) puis aligne
      // l'état de session comme une déconnexion volontaire.
      closeModal();
      await lock();
    } catch (e) {
      toast(
        'error',
        estMauvaisePhrase(e) ? t.onboarding.backupWrongPassphrase : t.errors.actionFailed,
      );
    } finally {
      setBusy(false);
    }
  };

  const doImport = async (): Promise<void> => {
    setBusy(true);
    try {
      const compte = await backupImport(phrase);
      // Sélecteur annulé : aucun compte créé.
      if (compte === null) return;
      fermerSaisie();
      toast('info', ts.settings.backupImportDone);
    } catch (e) {
      toast(
        'error',
        estMauvaisePhrase(e) ? t.onboarding.backupWrongPassphrase : t.errors.actionFailed,
      );
    } finally {
      setBusy(false);
    }
  };

  const valider = (): void => void (mode === 'export' ? doExport() : doImport());

  return (
    <SettingsSection title={ts.settings.backupTitle} hint={ts.settings.backupHint}>
      <div className="rounded-lg bg-sidebar p-4">
        <p className="mb-4 rounded-md border-s-4 border-yellow bg-rail/60 px-3 py-2 text-sm leading-relaxed text-muted">
          {ts.settings.backupExportWarning}
        </p>
        {mode === null ? (
          <div className="flex flex-wrap gap-3">
            <button
              type="button"
              disabled={busy}
              onClick={() => setMode('export')}
              className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
            >
              {ts.settings.backupExport}
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => setMode('import')}
              className="rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
            >
              {ts.settings.backupImport}
            </button>
          </div>
        ) : (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              valider();
            }}
          >
            <label
              htmlFor="backup-passphrase"
              className="mb-1 block text-sm font-medium text-norm"
            >
              {mode === 'export'
                ? ts.settings.backupPassphrasePrompt
                : t.onboarding.backupPassphraseImportPrompt}
            </label>
            <input
              id="backup-passphrase"
              type="password"
              autoFocus
              value={phrase}
              disabled={busy}
              onChange={(e) => setPhrase(e.target.value)}
              className="w-full rounded-lg border border-input bg-input px-3 py-2 text-sm text-norm outline-none focus-visible:ring-2 focus-visible:ring-blurple"
            />
            {mode === 'import' && (
              <p className="mt-1 text-xs leading-relaxed text-faint">
                {t.onboarding.backupPassphraseImportHint}
              </p>
            )}
            <div className="mt-3 flex flex-wrap gap-3">
              <button
                type="submit"
                disabled={busy}
                className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
              >
                {t.onboarding.backupConfirm}
              </button>
              <button
                type="button"
                disabled={busy}
                onClick={fermerSaisie}
                className="rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
              >
                {t.onboarding.backupCancel}
              </button>
            </div>
          </form>
        )}
      </div>
    </SettingsSection>
  );
}

/**
 * Danger zone: logs out without quitting the app. Locking drops the node's
 * in-memory keys host-side and lands on the unlock screen, exactly like a
 * fresh launch — hence the inline confirmation before anything happens.
 */
function LogoutSection() {
  const t = useT();
  const ts = useSettingsT();
  const closeModal = useUi((s) => s.closeModal);
  const lock = useSession((s) => s.lock);
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);

  const logout = (): void => {
    if (busy) return;
    setBusy(true);
    // Close settings first: the unlock screen must never sit under a modal.
    closeModal();
    // `lock` reports failures through the session store, never rejects.
    void lock();
  };

  return (
    <SettingsSection title={ts.settings.dangerZoneTitle} hint={ts.settings.logoutHint}>
      <div className="rounded-lg border border-red/40 bg-sidebar p-4">
        {!confirming ? (
          <button
            type="button"
            onClick={() => setConfirming(true)}
            className="rounded-lg bg-red px-4 py-2 text-sm font-medium text-on-red transition-colors hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
          >
            {t.app.logout}
          </button>
        ) : (
          <div className="flex flex-wrap items-center gap-3">
            <p className="min-w-0 flex-1 text-sm text-norm">{t.app.logoutConfirmText}</p>
            <button
              type="button"
              disabled={busy}
              onClick={logout}
              className="rounded-lg bg-red px-4 py-2 text-sm font-medium text-on-red transition-colors hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
            >
              {t.app.logoutConfirm}
            </button>
            <button
              type="button"
              disabled={busy}
              onClick={() => setConfirming(false)}
              className="rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
            >
              {t.app.cancel}
            </button>
          </div>
        )}
      </div>
    </SettingsSection>
  );
}

export function AccountTab() {
  const t = useT();
  const ts = useSettingsT();
  const toast = useUi((s) => s.toast);
  const self = useSession((s) => s.self);
  const setName = useSession((s) => s.setName);
  const [draft, setDraft] = useState(self?.name ?? '');
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);

  if (!self) return null;

  const trimmed = draft.trim();
  const valid = isValidName(draft);
  const dirty = trimmed !== (self.name ?? '');
  const showInvalid = trimmed !== '' && !valid;

  const save = async (): Promise<void> => {
    if (!valid || !dirty || busy) return;
    setBusy(true);
    try {
      await setName(trimmed);
      toast('success', ts.settings.pseudonymSaved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  const copyCode = (): void => {
    void navigator.clipboard.writeText(self.friend_code).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), COPY_FEEDBACK_MS);
    });
  };

  return (
    <div>
      <AvatarSection />

      <BannerSection />

      <SettingsSection title={ts.settings.pseudonym} hint={ts.settings.pseudonymHint}>
        <div className="flex gap-3 rounded-lg bg-sidebar p-3">
          <input
            aria-label={ts.settings.pseudonym}
            placeholder={ts.settings.pseudonymPlaceholder}
            value={draft}
            maxLength={NAME_MAX + 8}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void save();
            }}
            className="min-w-0 flex-1 rounded-md border border-transparent bg-input px-3 py-2 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
          />
          <button
            type="button"
            disabled={!valid || !dirty || busy}
            onClick={() => void save()}
            className="shrink-0 rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
          >
            {ts.settings.pseudonymSave}
          </button>
        </div>
        {showInvalid && (
          <p className="mt-2 text-sm text-red">{ts.settings.pseudonymInvalid}</p>
        )}
      </SettingsSection>

      <PronounsSection />

      <BioSection />

      <ProfilePersonalization />

      <SettingsSection title={ts.settings.identity}>
        <div className="rounded-lg bg-sidebar p-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <div className="text-xs font-medium uppercase text-faint">
                {t.friends.myCode}
              </div>
              <div className="selectable truncate font-mono text-norm">
                {self.friend_code}
              </div>
            </div>
            <button
              type="button"
              onClick={copyCode}
              className="shrink-0 rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
            >
              {copied ? t.app.copied : ts.settings.copyFriendCode}
            </button>
          </div>
          <div className="mt-3 text-xs font-medium uppercase text-faint">
            {ts.settings.publicKey}
          </div>
          <div className="selectable font-mono text-xs text-muted">
            {abbreviate(self.pubkey)}
          </div>
        </div>
      </SettingsSection>

      <SettingsSection title={ts.settings.recoveryNoteTitle}>
        <p className="rounded-lg border-s-4 border-yellow bg-sidebar px-4 py-3 text-sm leading-relaxed text-muted">
          {ts.settings.recoveryNote}
        </p>
      </SettingsSection>

      <DevicesSection />

      <BackupSection />

      <LogoutSection />
    </div>
  );
}
