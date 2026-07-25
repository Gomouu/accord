/**
 * Bouton « Importer une sauvegarde » partagé par le sélecteur de comptes et
 * l'onboarding. Un clic révèle un champ de phrase de passe (facultatif : vide =
 * ancienne sauvegarde non chiffrée ≤ 3.4) puis appelle `backupImport`. Centralise
 * ce flux pour que les deux points d'entrée « importer comme compte neuf »
 * partagent exactement le même comportement et les mêmes messages.
 */

import { useState } from 'react';
import { backupImport } from '../lib/bridge';
import { useT } from '../stores/ui';

/** Détecte la « mauvaise phrase de passe » dans le message d'erreur de l'hôte. */
function estMauvaisePhrase(erreur: unknown): boolean {
  const message = erreur instanceof Error ? erreur.message : String(erreur);
  return message.includes('secret de déverrouillage incorrect');
}

type Props = {
  /** Appelée après un import réussi (recharger la liste, naviguer…). */
  onImported: () => Promise<void> | void;
  /** Notifie l'utilisateur (réutilise le `toast` de l'appelant). */
  onToast: (kind: 'error' | 'info', text: string) => void;
  /** Classe du bouton déclencheur (styles propres à chaque écran). */
  className?: string;
  /** Rendu du contenu du bouton déclencheur (icône + label de l'écran). */
  children: React.ReactNode;
};

export function BackupImportButton({ onImported, onToast, className, children }: Props) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const [phrase, setPhrase] = useState('');
  const [busy, setBusy] = useState(false);

  const fermer = (): void => {
    setOpen(false);
    setPhrase('');
  };

  const lancer = async (): Promise<void> => {
    setBusy(true);
    try {
      const compte = await backupImport(phrase);
      // Sélecteur natif annulé : aucun compte créé, on garde le champ ouvert.
      if (compte === null) return;
      fermer();
      onToast('info', t.onboarding.importBackupDone);
      await onImported();
    } catch (e) {
      onToast(
        'error',
        estMauvaisePhrase(e)
          ? t.onboarding.backupWrongPassphrase
          : e instanceof Error
            ? e.message
            : String(e),
      );
    } finally {
      setBusy(false);
    }
  };

  if (!open) {
    return (
      <button type="button" className={className} onClick={() => setOpen(true)}>
        {children}
      </button>
    );
  }

  return (
    <form
      className="mt-2 w-full"
      onSubmit={(e) => {
        e.preventDefault();
        void lancer();
      }}
    >
      <label
        htmlFor="backup-import-passphrase"
        className="mb-1 block text-sm font-medium text-norm"
      >
        {t.onboarding.backupPassphraseImportPrompt}
      </label>
      <input
        id="backup-import-passphrase"
        type="password"
        autoFocus
        value={phrase}
        disabled={busy}
        onChange={(e) => setPhrase(e.target.value)}
        className="w-full rounded-lg border border-input bg-input px-3 py-2 text-sm text-norm outline-none focus-visible:ring-2 focus-visible:ring-blurple"
      />
      <p className="mt-1 text-xs leading-relaxed text-faint">
        {t.onboarding.backupPassphraseImportHint}
      </p>
      <div className="mt-3 flex gap-3">
        <button
          type="submit"
          disabled={busy}
          className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple disabled:opacity-50"
        >
          {t.onboarding.backupConfirm}
        </button>
        <button
          type="button"
          disabled={busy}
          onClick={fermer}
          className="rounded-lg bg-rail px-4 py-2 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple disabled:opacity-50"
        >
          {t.onboarding.backupCancel}
        </button>
      </div>
    </form>
  );
}
