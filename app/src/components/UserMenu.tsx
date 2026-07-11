/**
 * Menu utilisateur rapide (façon Discord), ouvert au clic sur l'avatar/pseudo
 * du panneau utilisateur : mini carte de profil en tête (bannière, avatar,
 * pseudo teinté par la couleur d'accent, pronoms, snippet de statut
 * personnalisé ou de bio — même lecture que `ProfilePopover` pour soi-même),
 * puis les sections existantes : bascule de statut (même présence riche que
 * `friends.setOwnStatus`, y compris le texte personnalisé), copie de son
 * propre ID, changement de compte, et déconnexion rapide — sans passer par
 * les Paramètres. La déconnexion garde la même confirmation en deux temps
 * que `AccountTab.LogoutSection` (inline, pas de modal supplémentaire) ; le
 * changement de compte est une action normale (pas de style « danger »).
 *
 * Navigation clavier façon `ContextMenu` : flèches haut/bas déplacent le
 * focus parmi les items « bouton » (roving tabindex), Entrée/Espace les
 * activent nativement. Le champ de statut personnalisé garde son
 * comportement de saisie normal (les flèches n'y sont pas interceptées).
 * Fermeture au clic extérieur et à Échap, comme `StatusMenu`/`ProfilePopover`.
 *
 * L'en-tête (bannière + avatar qui chevauche) reprend la technique de
 * `ProfilePopover` : `overflow-hidden` sur le panneau entier fait que la
 * bannière garde les coins arrondis, tandis que l'avatar — positionné par une
 * marge négative, mais toujours dans les bornes du panneau — n'est jamais
 * rogné malgré le chevauchement (`relative z-10` + `ring-4 ring-modal` pour
 * le détacher visuellement de la bannière).
 */

import { useEffect, useRef, useState } from 'react';
import type { OwnPresenceStatus, PresenceStatus } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
import { profileCardGradient, profileColorCss } from '../lib/color';
import { useFriends } from '../stores/friends';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { Avatar } from './Avatar';
import { CheckMenuIcon, CloseIcon, CopyMenuIcon, LeaveMenuIcon } from './ContextMenu';
import { PresenceDot } from './PresenceDot';
import { ProfileBanner } from './ProfileBanner';

/** Statut affichable de son propre nœud (invisible = pastille hors ligne). */
export function ownDotStatus(status: OwnPresenceStatus): PresenceStatus {
  return status === 'invisible' ? 'offline' : status;
}

/** Icône « changer de compte » (voir ICON SPEC, styles/global.css). */
function SwitchAccountIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="m16 3 4 4-4 4" />
      <path d="M20 7H4" />
      <path d="m8 21-4-4 4-4" />
      <path d="M4 17h16" />
    </svg>
  );
}

export function UserMenu({ onClose }: { onClose: () => void }) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const closeModal = useUi((s) => s.closeModal);
  const self = useSession((s) => s.self);
  const lock = useSession((s) => s.lock);
  const switchAccount = useSession((s) => s.switchAccount);
  const ownStatus = useFriends((s) => s.ownStatus);
  const ownStatusText = useFriends((s) => s.ownStatusText);
  const setOwnStatus = useFriends((s) => s.setOwnStatus);
  const [draft, setDraft] = useState(ownStatusText ?? '');
  const [confirmingLogout, setConfirmingLogout] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const itemRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const [activeIndex, setActiveIndex] = useState(-1);

  // Fermeture au clic extérieur et à Échap (même approche que ProfilePopover).
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') onClose();
    };
    const onDown = (e: MouseEvent): void => {
      if (ref.current !== null && !ref.current.contains(e.target as Node)) onClose();
    };
    window.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onDown);
    };
  }, [onClose]);

  useEffect(() => {
    ref.current?.focus();
  }, []);

  if (self === null) return null;

  const displayName = selfDisplayName(self);
  const accentHex = profileColorCss(self.accent_color);
  // Même fond thématisé que ProfilePopover (teinte bannière, repli accent).
  const cardGradient = profileCardGradient(self.banner_color ?? self.accent_color);
  // Snippet compact façon ProfilePopover : statut personnalisé en priorité,
  // repli sur la bio tant qu'aucun statut personnalisé n'est défini.
  const snippet = (ownStatusText ?? '') !== '' ? ownStatusText : self.bio;

  const applyStatus = (status: OwnPresenceStatus, custom?: string): void => {
    setOwnStatus(status, custom).catch(() => toast('error', t.errors.actionFailed));
  };

  const copyId = (): void => {
    copyToClipboard(
      self.pubkey,
      () => toast('info', t.app.copied),
      () => toast('error', t.errors.actionFailed),
    );
    onClose();
  };

  const copyFriendCode = (): void => {
    copyToClipboard(
      self.friend_code,
      () => toast('info', t.app.copied),
      () => toast('error', t.errors.actionFailed),
    );
  };

  const doSwitchAccount = (): void => {
    closeModal();
    void switchAccount();
  };

  // Même geste que `LogoutSection` (Paramètres) : ferme la modale éventuelle
  // avant de verrouiller — `lock` rapporte ses échecs via le store, ne rejette
  // jamais.
  const confirmLogout = (): void => {
    closeModal();
    void lock();
  };

  const options: { status: OwnPresenceStatus; label: string }[] = [
    { status: 'online', label: t.profil.online },
    { status: 'idle', label: t.profil.idle },
    { status: 'dnd', label: t.profil.dnd },
    { status: 'invisible', label: t.profil.invisible },
  ];

  // Liste plate façon roving-tabindex (voir `ContextMenu`) : statuts, copie
  // d'ID, changement de compte, puis déconnexion (ou sa confirmation à deux
  // boutons). L'index s'incrémente au fil du rendu, dans l'ordre du DOM.
  let cursor = -1;
  const nextIndex = (): number => {
    cursor += 1;
    return cursor;
  };
  const itemCount = options.length + 2 + (confirmingLogout ? 2 : 1);

  const moveActive = (next: number): void => {
    const bounded = ((next % itemCount) + itemCount) % itemCount;
    setActiveIndex(bounded);
    itemRefs.current[bounded]?.focus();
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLDivElement>): void => {
    // Ignore les flèches quand le focus est dans le champ de statut
    // personnalisé : il doit garder son comportement de saisie normal.
    if (e.target instanceof HTMLInputElement) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      moveActive(activeIndex + 1);
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      moveActive(activeIndex - 1);
    }
  };

  return (
    <div
      ref={ref}
      role="menu"
      aria-label={t.profil.userMenu}
      tabIndex={-1}
      onKeyDown={onKeyDown}
      className="glass-strong context-menu-enter absolute bottom-[calc(100%+8px)] left-2 z-50 w-[300px] overflow-hidden rounded-lg focus:outline-none"
    >
      {/* Mini carte de profil — voir la note de chevauchement en tête de fichier. */}
      <ProfileBanner
        hash={self.banner}
        hint={self.pubkey}
        color={self.banner_color}
        heightClassName="h-14"
      />
      <div
        className="-mt-7 px-3 pb-3"
        style={cardGradient !== null ? { backgroundImage: cardGradient } : undefined}
      >
        <div className="relative z-10 mb-1.5 inline-flex rounded-full ring-4 ring-modal">
          <Avatar
            id={self.pubkey}
            name={displayName}
            size={56}
            avatarHash={self.avatar}
            hint={self.pubkey}
          />
        </div>
        <div
          className="truncate text-base font-semibold text-header"
          style={accentHex !== null ? { color: accentHex } : undefined}
        >
          {displayName}
        </div>
        {self.pronouns !== null && self.pronouns !== '' && (
          <p className="truncate text-xs text-muted">{self.pronouns}</p>
        )}
        {snippet !== null && snippet !== '' && (
          <p className="mt-0.5 truncate text-xs text-muted">{snippet}</p>
        )}
        <div className="mt-1 flex items-center gap-1.5">
          <span className="selectable truncate font-mono text-xs text-faint">
            {self.friend_code}
          </span>
          <button
            type="button"
            aria-label={t.profil.copyFriendCode}
            title={t.profil.copyFriendCode}
            onClick={copyFriendCode}
            className="flex h-5 w-5 shrink-0 items-center justify-center rounded-xs text-faint transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-1 focus-visible:ring-offset-modal active:scale-95"
          >
            <CopyMenuIcon />
          </button>
        </div>
      </div>

      <div className="p-1.5 pt-0">
        <div className="my-1 h-px bg-input/60" role="separator" />

        <div className="px-2 pb-1 text-[10px] font-medium uppercase tracking-wide text-faint">
          {t.profil.setStatus}
        </div>
        {options.map(({ status, label }) => {
          const index = nextIndex();
          const checked = ownStatus === status;
          return (
            <button
              key={status}
              ref={(el) => {
                itemRefs.current[index] = el;
              }}
              type="button"
              role="menuitemradio"
              aria-checked={checked}
              tabIndex={index === activeIndex ? 0 : -1}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={() => {
                applyStatus(status);
                onClose();
              }}
              className={`flex h-9 w-full items-center gap-2.5 rounded-md px-2.5 text-sm transition-colors duration-fast focus-visible:outline-none ${
                checked
                  ? 'bg-chat-hover text-header'
                  : 'text-norm hover:bg-chat-hover focus-visible:bg-chat-hover'
              }`}
            >
              <PresenceDot status={ownDotStatus(status)} />
              <span className="min-w-0 flex-1 truncate text-left">{label}</span>
              {checked && (
                <span
                  aria-hidden
                  className="flex h-[18px] w-[18px] shrink-0 items-center justify-center text-header"
                >
                  <CheckMenuIcon />
                </span>
              )}
            </button>
          );
        })}
        <div className="flex items-center gap-1.5 px-1 py-1.5">
          <input
            aria-label={t.profil.customStatusPlaceholder}
            placeholder={t.profil.customStatusPlaceholder}
            value={draft}
            maxLength={128}
            onChange={(e) => setDraft(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                applyStatus(ownStatus, draft);
                onClose();
              }
            }}
            className="min-w-0 flex-1 rounded-md border border-transparent bg-input px-2 py-1.5 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
          />
          {(ownStatusText ?? '') !== '' && (
            <button
              type="button"
              aria-label={t.profil.clearCustomStatus}
              title={t.profil.clearCustomStatus}
              onClick={() => {
                applyStatus(ownStatus, '');
                onClose();
              }}
              className="rounded-md p-1.5 text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
            >
              <CloseIcon size={14} />
            </button>
          )}
        </div>
        <div className="px-1 pb-1 text-[10px] text-faint">
          {t.profil.customStatusHint}
        </div>

        <div className="my-1.5 h-px bg-input/60" role="separator" />

        {(() => {
          const index = nextIndex();
          return (
            <button
              key="copy-id"
              ref={(el) => {
                itemRefs.current[index] = el;
              }}
              type="button"
              role="menuitem"
              tabIndex={index === activeIndex ? 0 : -1}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={copyId}
              className="flex h-9 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-sm font-medium text-norm transition-colors duration-fast hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none"
            >
              <span
                aria-hidden
                className="flex h-[18px] w-[18px] shrink-0 items-center justify-center"
              >
                <CopyMenuIcon />
              </span>
              {t.profil.copyMyId}
            </button>
          );
        })()}

        {(() => {
          const index = nextIndex();
          return (
            <button
              key="switch-account"
              ref={(el) => {
                itemRefs.current[index] = el;
              }}
              type="button"
              role="menuitem"
              tabIndex={index === activeIndex ? 0 : -1}
              onMouseEnter={() => setActiveIndex(index)}
              onClick={doSwitchAccount}
              className="flex h-9 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-sm font-medium text-norm transition-colors duration-fast hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none"
            >
              <span
                aria-hidden
                className="flex h-[18px] w-[18px] shrink-0 items-center justify-center"
              >
                <SwitchAccountIcon />
              </span>
              {t.profil.switchAccount}
            </button>
          );
        })()}

        <div className="my-1.5 h-px bg-input/60" role="separator" />

        {!confirmingLogout ? (
          (() => {
            const index = nextIndex();
            return (
              <button
                key="logout"
                ref={(el) => {
                  itemRefs.current[index] = el;
                }}
                type="button"
                role="menuitem"
                tabIndex={index === activeIndex ? 0 : -1}
                onMouseEnter={() => setActiveIndex(index)}
                onClick={() => setConfirmingLogout(true)}
                className="flex h-9 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-sm font-medium text-red transition-colors duration-fast hover:bg-red/10 focus-visible:bg-red/10 focus-visible:outline-none"
              >
                <span
                  aria-hidden
                  className="flex h-[18px] w-[18px] shrink-0 items-center justify-center"
                >
                  <LeaveMenuIcon />
                </span>
                {t.settings.logout}
              </button>
            );
          })()
        ) : (
          <div className="rounded-lg bg-rail/40 p-2">
            <p className="text-xs text-norm">{t.settings.logoutConfirmText}</p>
            <div className="mt-2 flex gap-1.5">
              {(() => {
                const confirmIndex = nextIndex();
                const cancelIndex = nextIndex();
                return (
                  <>
                    <button
                      key="logout-confirm"
                      ref={(el) => {
                        itemRefs.current[confirmIndex] = el;
                      }}
                      type="button"
                      tabIndex={confirmIndex === activeIndex ? 0 : -1}
                      onMouseEnter={() => setActiveIndex(confirmIndex)}
                      onClick={confirmLogout}
                      className="flex-1 rounded-sm bg-red px-2 py-1.5 text-xs font-medium text-white transition-opacity duration-fast hover:opacity-90 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
                    >
                      {t.settings.logoutConfirm}
                    </button>
                    <button
                      key="logout-cancel"
                      ref={(el) => {
                        itemRefs.current[cancelIndex] = el;
                      }}
                      type="button"
                      tabIndex={cancelIndex === activeIndex ? 0 : -1}
                      onMouseEnter={() => setActiveIndex(cancelIndex)}
                      onClick={() => setConfirmingLogout(false)}
                      className="rounded-sm bg-rail px-2 py-1.5 text-xs font-medium text-norm transition-colors duration-fast hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
                    >
                      {t.app.cancel}
                    </button>
                  </>
                );
              })()}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
