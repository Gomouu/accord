/**
 * Sélecteur de comptes — écran d'accueil dès 2 comptes locaux connus, ou
 * atteint volontairement depuis l'écran de déverrouillage à compte unique
 * via « Changer de compte » (voir `session.goToWelcome`). Une ligne par
 * compte façon Discord : avatar coloré aux initiales (aucune image
 * disponible avant déverrouillage — voir `Avatar`, `avatarHash: null`),
 * pseudo, préfixe de clé publique, dernière utilisation ; clic → invite de
 * phrase de passe pour CE compte (`account_unlock`).
 *
 * « Ajouter un compte » et « Importer depuis une phrase de récupération »
 * réutilisent `CreateForm`/`RestoreForm` (screens/Onboarding.tsx) telles
 * quelles, câblées sur `createAccount`/`restoreAccount` — jamais sur le
 * profil actif courant (contrat hôte : ces commandes créent toujours un
 * répertoire de profil dédié).
 *
 * Un échec de déverrouillage fait transiter la phase par `starting` avant
 * de revenir à `welcome` (même mécanique que `UnlockForm`) : ce composant
 * est alors démonté puis remonté par `App`, ce qui efface l'état local
 * (ligne dépliée, phrase saisie) — c'est pourquoi l'erreur (portée par le
 * store, qui survit au remontage) s'affiche en bandeau au-dessus de la
 * liste plutôt que scopée à une ligne qui n'existe plus après remontage.
 */

import { useState } from 'react';
import { Avatar } from '../components/Avatar';
import { interpolate, type Dict, type Lang } from '../i18n';
import type { AccountMeta } from '../lib/bridge';
import { formatTimestamp } from '../lib/format';
import { useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { CreateForm, RestoreForm } from './Onboarding';
import { Card, PrimaryButton } from './onboardingUi';

/** Sous-titre d'une ligne compte : préfixe de clé publique · dernière utilisation. */
function accountSubtitle(account: AccountMeta, lang: Lang, t: Dict): string {
  const parts: string[] = [];
  if (account.pubkey_short !== null) parts.push(account.pubkey_short);
  if (account.last_used_ms > 0) {
    parts.push(
      interpolate(t.onboarding.accountLastUsed, {
        date: formatTimestamp(account.last_used_ms, lang),
      }),
    );
  }
  return parts.join(' · ');
}

/** Ligne « compte » : identité au clic, invite de phrase de passe repliable. */
function AccountRow({
  account,
  open,
  onToggle,
  onUnlock,
}: {
  account: AccountMeta;
  open: boolean;
  onToggle: () => void;
  onUnlock: (passphrase: string) => void;
}) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const [pass, setPass] = useState('');

  return (
    <li className="overflow-hidden rounded-md bg-rail/60">
      <button
        type="button"
        onClick={onToggle}
        aria-expanded={open}
        aria-label={interpolate(t.onboarding.unlockAccountLabel, { name: account.name })}
        className="flex w-full items-center gap-3 px-3 py-2.5 text-left transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-rail"
      >
        <Avatar id={account.id} name={account.name} size={40} avatarHash={null} />
        <div className="min-w-0 flex-1">
          <div className="truncate text-sm font-medium text-header">{account.name}</div>
          <div className="truncate font-mono text-xs text-faint">
            {accountSubtitle(account, lang, t)}
          </div>
        </div>
      </button>
      {open && (
        <form
          className="border-t border-[color:var(--glass-border)] px-3 py-3"
          onSubmit={(e) => {
            e.preventDefault();
            onUnlock(pass);
          }}
        >
          <label className="mb-2 block">
            <span className="mb-1 block text-xs font-semibold uppercase tracking-wide text-muted">
              {t.onboarding.passphrase}
            </span>
            <input
              type="password"
              autoFocus
              value={pass}
              onChange={(e) => setPass(e.target.value)}
              className="w-full rounded-md border border-transparent bg-input px-2.5 py-2 text-sm text-norm outline-none transition-colors duration-fast focus:border-blurple/50"
            />
          </label>
          <div className="flex gap-2">
            <button
              type="submit"
              disabled={pass.length === 0}
              className="flex-1 rounded-sm bg-blurple px-3 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover disabled:pointer-events-none disabled:opacity-50 active:scale-[0.98]"
            >
              {t.onboarding.unlock}
            </button>
            <button
              type="button"
              onClick={onToggle}
              className="rounded-sm bg-rail px-3 py-1.5 text-sm font-medium text-norm transition-colors duration-fast hover:bg-input"
            >
              {t.app.cancel}
            </button>
          </div>
        </form>
      )}
    </li>
  );
}

type Mode = 'list' | 'create' | 'restore';

export function AccountPicker() {
  const t = useT();
  const accounts = useSession((s) => s.accounts);
  const unlockAccount = useSession((s) => s.unlockAccount);
  const createAccount = useSession((s) => s.createAccount);
  const restoreAccount = useSession((s) => s.restoreAccount);
  const error = useSession((s) => s.error);
  const [mode, setMode] = useState<Mode>('list');
  const [openId, setOpenId] = useState<string | null>(null);

  if (mode === 'create') {
    return (
      <CreateForm
        onSubmit={createAccount}
        onRestore={() => setMode('restore')}
        onCancel={() => setMode('list')}
      />
    );
  }
  if (mode === 'restore') {
    return (
      <RestoreForm
        onSubmit={restoreAccount}
        onBack={() => setMode('create')}
        onCancel={() => setMode('list')}
      />
    );
  }

  return (
    <Card wide>
      <h1 className="mb-2 text-center text-2xl font-bold text-header">
        {t.onboarding.welcomeTitle}
      </h1>
      <p className="mb-5 text-center text-sm text-muted">{t.onboarding.welcomeHint}</p>
      {error !== null && <p className="mb-4 text-center text-sm text-red">{error}</p>}
      {accounts.length > 0 && (
        <ul className="mb-5 flex max-h-64 flex-col gap-2 overflow-y-auto overscroll-contain pr-1">
          {accounts.map((account) => (
            <AccountRow
              key={account.id}
              account={account}
              open={openId === account.id}
              onToggle={() =>
                setOpenId((current) => (current === account.id ? null : account.id))
              }
              onUnlock={(passphrase) => void unlockAccount(account.id, passphrase)}
            />
          ))}
        </ul>
      )}
      <PrimaryButton label={t.onboarding.addAccount} onClick={() => setMode('create')} />
      <button
        type="button"
        onClick={() => setMode('restore')}
        className="mt-3 w-full text-center text-sm text-link hover:underline"
      >
        {t.onboarding.importPhrase}
      </button>
    </Card>
  );
}
