import { StrictMode, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import { Avatar } from '../components/Avatar';
import { AVATAR_DECORATIONS, PROFILE_EFFECTS, effectById } from '../lib/decorations';
import '../styles/global.css';
import '../styles/theme-scenes.css';
import '../styles/profile-personalization.css';
import '../styles/profile-personalization-extra.css';
import '../styles/profile-personalization-more.css';
import '../styles/profile-surfaces.css';

type ShowcaseTheme = 'dark' | 'light';

function ThemeButton({
  active,
  children,
  onClick,
}: {
  active: boolean;
  children: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-pressed={active}
      onClick={onClick}
      className={`rounded-full px-4 py-2 text-sm font-semibold focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal ${
        active
          ? 'bg-blurple text-white shadow-1'
          : 'bg-input text-muted hover:bg-chat-hover hover:text-norm'
      }`}
    >
      {children}
    </button>
  );
}

function DecorationGallery() {
  return (
    <section aria-labelledby="decorations-title">
      <div className="mb-5 flex items-end justify-between gap-4">
        <div>
          <p className="mb-1 text-xs font-semibold uppercase tracking-[0.18em] text-blurple">
            Collection intégrée
          </p>
          <h2 id="decorations-title" className="text-2xl font-semibold text-header">
            Décorations d’avatar
          </h2>
        </div>
        <span className="rounded-full bg-input px-3 py-1 text-xs font-medium text-muted">
          {AVATAR_DECORATIONS.length} cadres
        </span>
      </div>

      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 xl:grid-cols-6">
        {AVATAR_DECORATIONS.map((decoration) => (
          <article
            key={decoration.id}
            className="group flex min-h-40 flex-col items-center justify-center gap-4 rounded-lg border border-[color:var(--glass-border)] bg-sidebar p-4 shadow-1 transition-transform duration-fast hover:-translate-y-0.5 hover:shadow-2"
          >
            <Avatar
              id={`showcase-${decoration.id}`}
              name="Ari Vale"
              size={80}
              decoration={decoration.id}
            />
            <div className="w-full text-center">
              <h3 className="truncate text-sm font-semibold text-header">
                {decoration.label.fr}
              </h3>
              <p className="mt-0.5 truncate font-mono text-[10px] text-faint">
                {decoration.id}
              </p>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

function EffectGallery() {
  return (
    <section aria-labelledby="effects-title">
      <div className="mb-5 flex items-end justify-between gap-4">
        <div>
          <p className="mb-1 text-xs font-semibold uppercase tracking-[0.18em] text-blurple">
            Arrière-plans vivants
          </p>
          <h2 id="effects-title" className="text-2xl font-semibold text-header">
            Effets de profil
          </h2>
        </div>
        <span className="rounded-full bg-input px-3 py-1 text-xs font-medium text-muted">
          {PROFILE_EFFECTS.length} effets
        </span>
      </div>

      <div className="grid gap-3 sm:grid-cols-2">
        {PROFILE_EFFECTS.map((effect) => (
          <article
            key={effect.id}
            className="relative min-h-44 overflow-hidden rounded-lg border border-[color:var(--glass-border)] bg-modal p-5 shadow-1"
          >
            {effect.render()}
            <div className="relative flex h-full min-h-32 flex-col justify-between">
              <div className="flex items-center gap-3">
                <Avatar id={`effect-${effect.id}`} name="Noa Lin" size={44} />
                <div>
                  <h3 className="font-semibold text-header">{effect.label.fr}</h3>
                  <p className="font-mono text-[10px] text-faint">{effect.id}</p>
                </div>
              </div>
              <div className="mt-8">
                <div className="h-2 w-3/4 rounded-full bg-header/20" />
                <div className="mt-2 h-2 w-1/2 rounded-full bg-header/10" />
              </div>
            </div>
          </article>
        ))}
      </div>
    </section>
  );
}

function CompleteProfileCard() {
  const effect = effectById('aurora');

  return (
    <section aria-labelledby="profile-card-title">
      <div className="mb-5">
        <p className="mb-1 text-xs font-semibold uppercase tracking-[0.18em] text-blurple">
          Composition finale
        </p>
        <h2 id="profile-card-title" className="text-2xl font-semibold text-header">
          Carte de profil complète
        </h2>
      </div>

      <article className="glass-strong relative mx-auto max-w-sm overflow-hidden rounded-xl">
        {effect?.render()}
        <div className="relative h-28 bg-gradient-to-br from-blurple/80 via-[#7c5ce7] to-[#ef7ba8]">
          <div className="absolute inset-0 bg-gradient-to-t from-modal/70 to-transparent" />
        </div>
        <div className="relative -mt-10 px-5 pb-5">
          <div className="mb-4 flex items-end justify-between">
            <div className="rounded-full bg-modal p-1.5 shadow-2">
              <Avatar
                id="complete-profile"
                name="Ari Vale"
                size={80}
                decoration="aurora_ring"
              />
            </div>
            <span className="mb-1 inline-flex items-center gap-2 rounded-full bg-modal/80 px-3 py-1.5 text-xs font-medium text-muted shadow-1">
              <span className="h-2.5 w-2.5 rounded-full bg-green ring-2 ring-modal" />
              En ligne
            </span>
          </div>

          <div className="rounded-lg border border-[color:var(--glass-border)] bg-sidebar/90 p-4 shadow-1">
            <div className="flex items-center gap-2">
              <h3 className="truncate text-xl font-semibold text-header">Ari Vale</h3>
              <span className="rounded-full bg-blurple/15 px-2 py-0.5 text-[10px] font-bold uppercase tracking-wide text-blurple">
                Pionnier
              </span>
            </div>
            <p className="mt-0.5 text-xs text-muted">iel / elle</p>
            <p className="mt-3 text-sm leading-relaxed text-norm">
              Je construis des espaces calmes où les conversations restent entre nous.
            </p>

            <div className="my-4 h-px bg-input/70" role="separator" />

            <p className="text-[11px] font-semibold uppercase tracking-[0.14em] text-faint">
              Rôles
            </p>
            <div className="mt-2 flex flex-wrap gap-2">
              <span className="rounded-full bg-blurple/15 px-2.5 py-1 text-xs font-medium text-blurple">
                Design
              </span>
              <span className="rounded-full bg-green/15 px-2.5 py-1 text-xs font-medium text-green">
                Disponible
              </span>
            </div>

            <button
              type="button"
              className="mt-5 w-full rounded-full bg-blurple px-4 py-2.5 text-sm font-semibold text-white shadow-1 transition-transform duration-fast hover:-translate-y-0.5 hover:bg-blurple-hover active:translate-y-0 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
            >
              Envoyer un message
            </button>
          </div>
        </div>
      </article>
    </section>
  );
}

export function ProfilePersonalizationDemo() {
  const [theme, setTheme] = useState<ShowcaseTheme>('dark');
  const [reduceMotion, setReduceMotion] = useState(false);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = theme;
    if (reduceMotion) root.dataset.motion = 'reduce';
    else delete root.dataset.motion;
    return () => {
      delete root.dataset.theme;
      delete root.dataset.motion;
    };
  }, [theme, reduceMotion]);

  return (
    <main className="h-full overflow-auto bg-chat text-norm">
      <div className="app-ambient min-h-screen px-5 py-8 sm:px-8 lg:px-12">
        <div className="mx-auto max-w-7xl">
          <header className="glass-strong mb-10 flex flex-col gap-6 rounded-xl p-6 sm:flex-row sm:items-center sm:justify-between sm:p-8">
            <div className="max-w-2xl">
              <p className="mb-2 text-xs font-bold uppercase tracking-[0.22em] text-blurple">
                Accord · laboratoire visuel
              </p>
              <h1 className="text-3xl font-semibold tracking-tight text-header sm:text-4xl">
                Identité et personnalisation
              </h1>
              <p className="mt-3 max-w-xl text-sm leading-relaxed text-muted sm:text-base">
                Un inventaire sans backend des cadres, effets et états de profil, prêt
                pour les captures en thème sombre ou clair.
              </p>
            </div>

            <div className="flex flex-wrap items-center gap-2">
              <div
                role="group"
                aria-label="Thème de l’aperçu"
                className="flex rounded-full bg-modal/70 p-1 shadow-1"
              >
                <ThemeButton active={theme === 'dark'} onClick={() => setTheme('dark')}>
                  Sombre
                </ThemeButton>
                <ThemeButton active={theme === 'light'} onClick={() => setTheme('light')}>
                  Clair
                </ThemeButton>
              </div>
              <button
                type="button"
                aria-pressed={reduceMotion}
                onClick={() => setReduceMotion((value) => !value)}
                className="rounded-full bg-input px-4 py-2 text-sm font-semibold text-muted hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
              >
                Mouvement réduit {reduceMotion ? 'activé' : 'désactivé'}
              </button>
            </div>
          </header>

          <div className="space-y-12">
            <DecorationGallery />
            <div className="grid gap-10 xl:grid-cols-[minmax(0,1.5fr)_minmax(320px,0.8fr)]">
              <EffectGallery />
              <CompleteProfileCard />
            </div>
          </div>
        </div>
      </div>
    </main>
  );
}

const root = document.getElementById('root');
if (root === null) throw new Error('élément racine introuvable');

createRoot(root).render(
  <StrictMode>
    <ProfilePersonalizationDemo />
  </StrictMode>,
);
