/**
 * Onglet Mon compte : avatar (image recadrée en carré, réduite à 256 px,
 * publiée via profile.set_avatar), bannière, pseudo, pronoms et bio
 * (profile.set), couleurs de profil (accent/bannière, profile.set tri-état),
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
import { useUi, useT } from '../../stores/ui';
import { lireFichier } from '../../lib/files';
import { AvatarCropper } from '../AvatarCropper';
import { Avatar } from '../Avatar';
import { ColorSwatchPicker, SettingsSection } from './controls';
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
      toast('info', t.settings.avatarUpdated);
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
      toast('info', t.settings.avatarRemoved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsSection title={t.settings.avatarTitle} hint={t.settings.avatarHint}>
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
          aria-label={t.settings.avatarChoose}
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
          {t.settings.avatarChoose}
        </button>
        {(self.avatar !== null || preview !== null) && (
          <button
            type="button"
            disabled={busy}
            onClick={() => void remove()}
            className="rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
          >
            {t.settings.avatarRemove}
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
      toast('info', t.settings.bannerUpdated);
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
      toast('info', t.settings.bannerRemoved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsSection title={t.settings.bannerTitle} hint={t.settings.bannerHint}>
      <div className="rounded-lg bg-sidebar p-4">
        <BannerPreview
          preview={preview}
          hash={self.banner}
          hint={self.pubkey}
          label={t.settings.bannerTitle}
        />
        <input
          ref={fileRef}
          type="file"
          accept="image/*"
          aria-label={t.settings.bannerChoose}
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
            {t.settings.bannerChoose}
          </button>
          {(self.banner !== null || preview !== null) && (
            <button
              type="button"
              disabled={busy}
              onClick={() => void remove()}
              className="rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
            >
              {t.settings.bannerRemove}
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
      toast('info', t.settings.pronounsSaved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsSection title={t.settings.pronounsTitle} hint={t.settings.pronounsHint}>
      <div className="flex gap-3 rounded-lg bg-sidebar p-3">
        <input
          aria-label={t.settings.pronounsTitle}
          placeholder={t.settings.pronounsPlaceholder}
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
          {t.settings.pronounsSave}
        </button>
      </div>
    </SettingsSection>
  );
}

/** Section bio : zone de texte avec compteur, chaîne vide = effacer. */
function BioSection() {
  const t = useT();
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
      toast('info', t.settings.bioSaved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsSection title={t.settings.bioTitle} hint={t.settings.bioHint}>
      <div className="rounded-lg bg-sidebar p-3">
        <textarea
          aria-label={t.settings.bioTitle}
          placeholder={t.settings.bioPlaceholder}
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
            {t.settings.bioSave}
          </button>
        </div>
      </div>
    </SettingsSection>
  );
}

/** Section couleurs de profil : accent (teinte le pseudo) et bannière (repli sans image). */
function ColorsSection() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const self = useSession((s) => s.self);
  const setAccentColor = useSession((s) => s.setAccentColor);
  const setBannerColor = useSession((s) => s.setBannerColor);
  const [busyAccent, setBusyAccent] = useState(false);
  const [busyBanner, setBusyBanner] = useState(false);

  if (!self) return null;

  const pickAccent = async (color: number | null): Promise<void> => {
    if (busyAccent || color === self.accent_color) return;
    setBusyAccent(true);
    try {
      await setAccentColor(color);
      toast('info', t.settings.accentColorSaved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusyAccent(false);
    }
  };

  const pickBanner = async (color: number | null): Promise<void> => {
    if (busyBanner || color === self.banner_color) return;
    setBusyBanner(true);
    try {
      await setBannerColor(color);
      toast('info', t.settings.bannerColorSaved);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusyBanner(false);
    }
  };

  return (
    <SettingsSection title={t.settings.colorsTitle} hint={t.settings.colorsHint}>
      <div className="space-y-4 rounded-lg bg-sidebar p-4">
        <div>
          <div className="mb-2 text-xs font-medium uppercase text-faint">
            {t.settings.accentColorLabel}
          </div>
          <ColorSwatchPicker
            label={t.settings.accentColorLabel}
            value={self.accent_color}
            busy={busyAccent}
            onPick={(c) => void pickAccent(c)}
          />
        </div>
        <div className="h-px bg-input/60" role="separator" />
        <div>
          <div className="mb-2 text-xs font-medium uppercase text-faint">
            {t.settings.bannerColorLabel}
          </div>
          <p className="mb-2 text-xs text-muted">{t.settings.bannerColorHint}</p>
          <ColorSwatchPicker
            label={t.settings.bannerColorLabel}
            value={self.banner_color}
            busy={busyBanner}
            onPick={(c) => void pickBanner(c)}
          />
        </div>
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
    <SettingsSection title={t.settings.dangerZoneTitle} hint={t.settings.logoutHint}>
      <div className="rounded-lg border border-red/40 bg-sidebar p-4">
        {!confirming ? (
          <button
            type="button"
            onClick={() => setConfirming(true)}
            className="rounded-lg bg-red px-4 py-2 text-sm font-medium text-on-red transition-colors hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
          >
            {t.settings.logout}
          </button>
        ) : (
          <div className="flex flex-wrap items-center gap-3">
            <p className="min-w-0 flex-1 text-sm text-norm">
              {t.settings.logoutConfirmText}
            </p>
            <button
              type="button"
              disabled={busy}
              onClick={logout}
              className="rounded-lg bg-red px-4 py-2 text-sm font-medium text-on-red transition-colors hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-50"
            >
              {t.settings.logoutConfirm}
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
      toast('info', t.settings.pseudonymSaved);
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

      <SettingsSection title={t.settings.pseudonym} hint={t.settings.pseudonymHint}>
        <div className="flex gap-3 rounded-lg bg-sidebar p-3">
          <input
            aria-label={t.settings.pseudonym}
            placeholder={t.settings.pseudonymPlaceholder}
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
            {t.settings.pseudonymSave}
          </button>
        </div>
        {showInvalid && (
          <p className="mt-2 text-sm text-red">{t.settings.pseudonymInvalid}</p>
        )}
      </SettingsSection>

      <PronounsSection />

      <BioSection />

      <ColorsSection />

      <ProfilePersonalization />

      <SettingsSection title={t.settings.identity}>
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
              {copied ? t.app.copied : t.settings.copyFriendCode}
            </button>
          </div>
          <div className="mt-3 text-xs font-medium uppercase text-faint">
            {t.settings.publicKey}
          </div>
          <div className="selectable font-mono text-xs text-muted">
            {abbreviate(self.pubkey)}
          </div>
        </div>
      </SettingsSection>

      <SettingsSection title={t.settings.recoveryNoteTitle}>
        <p className="rounded-lg border-l-4 border-yellow bg-sidebar px-4 py-3 text-sm leading-relaxed text-muted">
          {t.settings.recoveryNote}
        </p>
      </SettingsSection>

      <LogoutSection />
    </div>
  );
}
