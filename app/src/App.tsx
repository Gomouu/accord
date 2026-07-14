/** Racine : aiguillage onboarding / application, bandeau hors-ligne, toasts. */

import { useEffect } from 'react';
import { AppShell } from './components/AppShell';
import { Toasts } from './components/Toasts';
import { AccountPicker } from './screens/AccountPicker';
import { ChooseNameScreen, Onboarding, RecoveryPhraseScreen } from './screens/Onboarding';
import { useSession } from './stores/session';
import { useT } from './stores/ui';

const LOGO_URL = new URL('./assets/accord-logo.svg', import.meta.url).href;

function BootScreen({ label }: { label: string }) {
  return (
    <div className="app-ambient flex h-full items-center justify-center bg-rail px-6">
      <div role="status" className="flex flex-col items-center text-center">
        <div className="relative mb-5">
          <span
            aria-hidden
            className="absolute -inset-3 rounded-[28px] border border-blurple/20 bg-blurple/10 shadow-2"
          />
          <img
            src={LOGO_URL}
            alt=""
            aria-hidden
            width={64}
            height={64}
            className="relative h-16 w-16 drop-shadow-lg"
          />
          <span
            aria-hidden
            className="absolute -inset-5 animate-spin rounded-[34px] border border-transparent border-t-blurple/80"
          />
        </div>
        <p className="text-sm font-medium text-muted">{label}</p>
        <span aria-hidden className="mt-3 flex gap-1.5">
          {[0, 1, 2].map((index) => (
            <span
              key={index}
              className="h-1.5 w-1.5 animate-pulse rounded-full bg-blurple"
              style={{ animationDelay: `${index * 120}ms` }}
            />
          ))}
        </span>
      </div>
    </div>
  );
}

export function App() {
  const t = useT();
  const phase = useSession((s) => s.phase);
  const recoveryPhrase = useSession((s) => s.recoveryPhrase);
  const askName = useSession((s) => s.askName);
  const init = useSession((s) => s.init);

  useEffect(() => {
    void init();
  }, [init]);

  if (phase === 'boot') {
    return <BootScreen label={t.app.loading} />;
  }

  if (phase === 'setup' || phase === 'locked' || phase === 'starting') {
    return (
      <>
        <Onboarding />
        <Toasts />
      </>
    );
  }

  if (phase === 'welcome') {
    return (
      <>
        <AccountPicker />
        <Toasts />
      </>
    );
  }

  if (recoveryPhrase !== null) {
    return <RecoveryPhraseScreen phrase={recoveryPhrase} />;
  }

  // Troisième écran d'accueil : pseudo (après création/restauration seulement).
  if (askName) {
    return (
      <>
        <ChooseNameScreen />
        <Toasts />
      </>
    );
  }

  return (
    <div className="flex h-full flex-col">
      {phase === 'offline' && (
        <div
          role="status"
          aria-live="polite"
          className="relative z-30 flex min-h-8 shrink-0 items-center justify-center gap-2 border-b border-white/10 bg-red/95 px-4 py-1.5 text-center text-sm font-medium text-on-red shadow-1"
        >
          <span className="relative flex h-2 w-2" aria-hidden>
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-white/70" />
            <span className="relative inline-flex h-2 w-2 rounded-full bg-white" />
          </span>
          <span className="text-pretty">{t.app.offline}</span>
        </div>
      )}
      <div className="min-h-0 flex-1">
        <AppShell />
      </div>
      <Toasts />
    </div>
  );
}
