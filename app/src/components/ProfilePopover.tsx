/**
 * Carte de profil façon Discord, ouverte au clic sur un pseudo ou un avatar
 * (messages, liste des membres). Affiche grand avatar, pseudo, pastille de
 * présence, bio et, en contexte de serveur, les rôles colorés du membre.
 * Actions : « Envoyer un message » (si ami) et « Bloquer ».
 * Se ferme au clic extérieur et à Échap ; positionnée près du déclencheur.
 */

import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { PresenceStatus } from '../lib/api';
import { profileCardGradient, profileColorCss } from '../lib/color';
import { effectById, frameById } from '../lib/decorations';
import { copyToClipboard } from '../lib/clipboard';
import { bouclerTab, focusables } from '../lib/focus';
import { displayNameOf, presenceOf, useFriends } from '../stores/friends';
import {
  nicknameOf,
  roleColorCss,
  serverAvatarOf,
  sortRoles,
  useGroups,
} from '../stores/groups';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT, type AncrePopover } from '../stores/ui';
import { api } from '../lib/client';
import { Avatar } from './Avatar';
import { CopyMenuIcon, EnvelopeMenuIcon } from './ContextMenu';
import { MarkdownText } from './MarkdownText';
import { PresenceDot } from './PresenceDot';
import { ProfileBanner } from './ProfileBanner';
import { UserMenu } from './UserMenu';

/** Largeur de la carte (px, façon Discord) ; sert au calcul de position initial. */
const CARD_WIDTH = 340;
/** Marge minimale au bord du viewport (px). */
const MARGE = 8;
const FRAME_OVERFLOW = 30;

/** Icône « retirer cet ami » (voir ICON SPEC, styles/global.css). */
function RemoveFriendIcon() {
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
      <path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" />
      <circle cx="9" cy="7" r="4" />
      <line x1="22" x2="16" y1="11" y2="11" />
    </svg>
  );
}

/** Icône « bloquer » (voir ICON SPEC, styles/global.css). */
function BlockUserIcon() {
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
      <circle cx="12" cy="12" r="10" />
      <path d="m4.9 4.9 14.2 14.2" />
    </svg>
  );
}

/** Position `fixed` (px) de la carte, calée près de l'ancre et bornée à l'écran. */
function calculerPosition(
  ancre: AncrePopover,
  largeur: number,
  hauteur: number,
): { left: number; top: number } {
  const vw = window.innerWidth;
  const vh = window.innerHeight;
  const gutter = vw >= largeur + (FRAME_OVERFLOW + MARGE) * 2 ? FRAME_OVERFLOW : 0;
  const bord = MARGE + gutter;
  const droite = ancre.right + MARGE;
  const gauche = ancre.left - MARGE - largeur;
  const left =
    droite + largeur <= vw - bord
      ? droite
      : gauche >= bord
        ? gauche
        : Math.max(bord, Math.min(ancre.left, vw - largeur - bord));
  // Sous l'ancre si la place le permet, au-dessus sinon.
  const enDessous = ancre.bottom + MARGE + hauteur <= vh - bord;
  const top = enDessous
    ? ancre.bottom + MARGE
    : Math.max(bord, ancre.top - MARGE - hauteur);
  return { left, top };
}

export function ProfilePopover() {
  const t = useT();
  const profile = useUi((s) => s.profile);
  const closeProfile = useUi((s) => s.closeProfile);
  const closeModal = useUi((s) => s.closeModal);
  const setView = useUi((s) => s.setView);
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const block = useFriends((s) => s.block);
  const removeFriend = useFriends((s) => s.remove);
  const ownStatus = useFriends((s) => s.ownStatus);
  const ownStatusText = useFriends((s) => s.ownStatusText);
  const openModal = useUi((s) => s.openModal);
  /** Confirmation en ligne du retrait d'ami (deux temps, distinct du blocage). */
  const [confirmRemove, setConfirmRemove] = useState(false);
  /** Note privée locale du contact (chargée à l'ouverture, jamais émise). */
  const [note, setNote] = useState('');
  const [noteLoaded, setNoteLoaded] = useState(false);
  const self = useSession((s) => s.self);
  const state = useGroups((s) =>
    profile?.groupId != null ? s.states[profile.groupId] : undefined,
  );
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);
  const ownTarget = profile !== null && self !== null && profile.pubkey === self.pubkey;
  const userMenuTarget = ownTarget && profile?.surface === 'user-menu';

  useLayoutEffect(() => {
    if (profile === null || userMenuTarget || ref.current === null) return undefined;
    const card = ref.current;
    const update = (): void =>
      setPos(calculerPosition(profile.ancre, card.offsetWidth, card.offsetHeight));
    update();
    const observer =
      typeof ResizeObserver === 'undefined' ? null : new ResizeObserver(() => update());
    observer?.observe(card);
    window.addEventListener('resize', update);
    return () => {
      observer?.disconnect();
      window.removeEventListener('resize', update);
    };
  }, [profile, userMenuTarget]);

  // Nouvelle cible : la confirmation de retrait en cours est abandonnée.
  useEffect(() => {
    setConfirmRemove(false);
  }, [profile]);

  // Charge la note privée locale du contact à l'ouverture (source de vérité :
  // le nœud). Purement locale — jamais envoyée au pair.
  useEffect(() => {
    setNote('');
    setNoteLoaded(false);
    if (profile === null) return undefined;
    const target = profile.pubkey;
    if (self !== null && target === self.pubkey) return undefined;
    let alive = true;
    api
      .friendsGetNote(target)
      .then(({ note: value }) => {
        if (alive) {
          setNote(value ?? '');
          setNoteLoaded(true);
        }
      })
      .catch(() => {
        if (alive) setNoteLoaded(true);
      });
    return () => {
      alive = false;
    };
  }, [profile, self]);

  useEffect(() => {
    if (profile === null || userMenuTarget) return undefined;
    const onKey = (e: KeyboardEvent): void => {
      if (e.key === 'Escape') closeProfile();
    };
    const onDown = (e: MouseEvent): void => {
      if (ref.current !== null && ref.current.contains(e.target as Node)) return;
      // Re-clic sur le déclencheur : ignoré ici pour que son onClick bascule
      // (referme) la carte au lieu que ce mousedown la referme puis que le
      // clic la rouvre (scintillement).
      const a = profile.ancre;
      if (
        e.clientX >= a.left &&
        e.clientX <= a.right &&
        e.clientY >= a.top &&
        e.clientY <= a.bottom
      ) {
        return;
      }
      closeProfile();
    };
    const onScroll = (e: Event): void => {
      if (e.target instanceof Node && ref.current?.contains(e.target)) return;
      closeProfile();
    };
    window.addEventListener('keydown', onKey);
    document.addEventListener('mousedown', onDown);
    document.addEventListener('scroll', onScroll, true);
    return () => {
      window.removeEventListener('keydown', onKey);
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('scroll', onScroll, true);
    };
  }, [profile, closeProfile, userMenuTarget]);

  // Focus pris par la carte à l'ouverture (piège Tab sur le conteneur), rendu
  // au déclencheur (avatar/pseudo cliqué) à la fermeture s'il existe encore.
  useEffect(() => {
    if (profile === null || userMenuTarget) return undefined;
    const declencheur =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    ref.current?.focus();
    return () => {
      if (declencheur !== null && declencheur.isConnected) declencheur.focus();
    };
  }, [profile, userMenuTarget]);

  if (profile === null) return null;

  const pubkey = profile.pubkey;
  const isSelf = self !== null && pubkey === self.pubkey;
  if (isSelf && profile.surface === 'user-menu') {
    return <UserMenu onClose={closeProfile} anchor={profile.ancre} />;
  }
  const contact = contacts.find((c) => c.pubkey === pubkey);
  // Pseudo de serveur prioritaire (contexte de groupe), sinon pseudo global.
  const name =
    nicknameOf(state, pubkey) ??
    (isSelf && self !== null ? selfDisplayName(self) : displayNameOf(contacts, pubkey));
  const globalAvatarHash =
    isSelf && self !== null ? self.avatar : (contact?.avatar ?? null);
  // Contexte serveur : l'avatar self-service prime sur l'avatar global.
  const avatarHash =
    state !== undefined
      ? (serverAvatarOf(state, contacts, pubkey) ?? globalAvatarHash)
      : globalAvatarHash;
  const bannerHash = isSelf && self !== null ? self.banner : (contact?.banner ?? null);
  const bio = isSelf && self !== null ? self.bio : (contact?.bio ?? null);
  const pronouns = isSelf && self !== null ? self.pronouns : (contact?.pronouns ?? null);
  const accentColor =
    isSelf && self !== null ? self.accent_color : (contact?.accent_color ?? null);
  const bannerColor =
    isSelf && self !== null ? self.banner_color : (contact?.banner_color ?? null);
  const avatarDecoration =
    isSelf && self !== null
      ? self.avatar_decoration
      : (contact?.avatar_decoration ?? null);
  const profileEffectId =
    isSelf && self !== null ? self.profile_effect : (contact?.profile_effect ?? null);
  const profileFrameId =
    isSelf && self !== null ? self.profile_frame : (contact?.profile_frame ?? null);
  const effect = effectById(profileEffectId);
  const frame = frameById(profileFrameId);
  const accentHex = profileColorCss(accentColor);
  // Fond thématisé de la carte (façon Discord) : teinte subtile de la
  // couleur de bannière si connue, sinon de l'accent — `null` (aucune des
  // deux) garde le fond neutre habituel.
  const cardGradient = profileCardGradient(bannerColor ?? accentColor);
  // Code ami : affiché UNIQUEMENT sur son propre profil (décision historique
  // « sans code ami » sur les profils d'autrui : le code d'un ami ne doit pas
  // pouvoir être repartagé à un tiers sans son accord — il se transmet de la
  // main à la main).
  const friendCode = isSelf && self !== null ? self.friend_code : null;
  // Statut riche : le sien (invisible affiché hors ligne) ou celui annoncé
  // par l'ami ; `null` masque la pastille (présence inconnue : non-ami).
  const status: PresenceStatus | null = isSelf
    ? ownStatus === 'invisible'
      ? 'offline'
      : ownStatus
    : contact?.state === 'friend'
      ? presenceOf(contact)
      : null;
  const statusText = isSelf ? ownStatusText : (contact?.status_text ?? null);

  const member = state?.members.find((m) => m.pubkey === pubkey);
  const roles =
    state !== undefined && member !== undefined
      ? sortRoles(state.roles).filter((r) => member.roles.includes(r.role_id))
      : [];
  const isFounder = state?.founder === pubkey;

  const canMessage = !isSelf && contact?.state === 'friend';
  const canRemove = !isSelf && contact?.state === 'friend';
  const canBlock = !isSelf && contact !== undefined && contact.state !== 'blocked';

  const envoyerMessage = (): void => {
    closeModal();
    setView({ kind: 'dm', peer: pubkey });
  };

  const modifierProfil = (): void => {
    closeProfile();
    openModal({ kind: 'settings' });
  };

  const retirer = (): void => {
    removeFriend(pubkey).catch(() => toast('error', t.errors.actionFailed));
    closeProfile();
  };

  const bloquer = (): void => {
    block(pubkey).catch(() => toast('error', t.errors.actionFailed));
    closeProfile();
  };

  /** Enregistre la note privée (au blur) si elle a changé ; jamais émise. */
  const enregistrerNote = (): void => {
    if (!noteLoaded) return;
    api
      .friendsSetNote(pubkey, note.trim())
      .catch(() => toast('error', t.errors.actionFailed));
  };

  const canNote = !isSelf && contact !== undefined;

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label={t.profil.title}
      tabIndex={-1}
      onKeyDown={(e) => {
        if (e.key === 'Tab' && document.activeElement === ref.current) {
          const targets = focusables(ref.current);
          const target = e.shiftKey ? targets.at(-1) : targets[0];
          if (target !== undefined) {
            e.preventDefault();
            target.focus();
          }
        } else bouclerTab(e, ref.current);
      }}
      style={{
        position: 'fixed',
        left: pos?.left ?? profile.ancre.left,
        top: pos?.top ?? profile.ancre.bottom,
        width: CARD_WIDTH,
        maxWidth: 'calc(100vw - 16px)',
        visibility: pos === null ? 'hidden' : 'visible',
      }}
      className="profile-card-shell popover-enter z-50 origin-top focus:outline-none"
    >
      {frame?.render()}
      <div
        style={{ maxHeight: 'calc(100vh - 72px)' }}
        className="profile-card-canvas profile-card-shell__surface glass-strong overflow-y-auto overscroll-contain rounded-xl"
      >
        {effect?.render()}
        {cardGradient !== null && (
          <span
            aria-hidden
            className="profile-card-tint"
            style={{ backgroundImage: cardGradient }}
          />
        )}
        <div className="profile-card-content">
          <ProfileBanner hash={bannerHash} hint={pubkey} color={bannerColor} />
          <div className="-mt-10 px-4 pb-4">
            <div className="mb-2 flex items-end justify-between">
              <div className="relative z-10 rounded-full bg-modal p-1 shadow-2">
                <Avatar
                  id={pubkey}
                  name={name}
                  size={80}
                  avatarHash={avatarHash}
                  hint={pubkey}
                  decoration={avatarDecoration}
                />
              </div>
              {status !== null && (
                <span className="mb-1 flex items-center gap-1.5 rounded-full border border-[color:var(--glass-border)] bg-modal/75 px-2.5 py-1 text-xs font-medium text-muted shadow-1">
                  <PresenceDot status={status} />
                  {t.profil[status]}
                </span>
              )}
            </div>

            <div className="profile-card-surface relative overflow-hidden rounded-lg p-3">
              <div className="relative">
                {accentHex !== null && (
                  <div
                    aria-hidden
                    className="mb-2 h-1 w-10 rounded-full"
                    style={{ backgroundColor: accentHex }}
                  />
                )}
                <div className="flex items-center gap-2">
                  <span
                    className="truncate text-lg font-semibold text-header"
                    style={accentHex !== null ? { color: accentHex } : undefined}
                  >
                    {name}
                  </span>
                  {isFounder && (
                    <span className="shrink-0 text-[10px] uppercase text-yellow">
                      {t.groups.founder}
                    </span>
                  )}
                </div>
                {pronouns !== null && pronouns !== '' && (
                  <p className="mt-0.5 truncate text-xs text-muted">{pronouns}</p>
                )}
                {statusText !== null && statusText !== '' && (
                  <p className="mt-0.5 truncate text-sm text-muted">{statusText}</p>
                )}
                {friendCode !== null && friendCode !== '' && (
                  <div className="mt-1 flex items-center gap-1.5">
                    <span className="selectable truncate font-mono text-xs text-faint">
                      {friendCode}
                    </span>
                    <button
                      type="button"
                      aria-label={t.profil.copyFriendCode}
                      title={t.profil.copyFriendCode}
                      onClick={() =>
                        copyToClipboard(
                          friendCode,
                          () => toast('info', t.app.copied),
                          () => toast('error', t.errors.actionFailed),
                        )
                      }
                      className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xs text-faint transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-1 focus-visible:ring-offset-sidebar active:scale-95"
                    >
                      <CopyMenuIcon />
                    </button>
                  </div>
                )}

                {bio !== null && bio !== '' && (
                  <>
                    <div className="mt-3 h-px bg-input/60" role="separator" />
                    <div className="mt-2 whitespace-pre-wrap break-words text-sm text-norm">
                      <MarkdownText text={bio} />
                    </div>
                  </>
                )}

                {roles.length > 0 && (
                  <>
                    <div className="mt-3 h-px bg-input/60" role="separator" />
                    <div className="mt-2 text-xs font-medium uppercase tracking-wide text-faint">
                      {t.profil.rolesLabel}
                    </div>
                    <div className="mt-1.5 flex flex-wrap gap-1.5">
                      {roles.map((role) => {
                        const couleur =
                          role.color === 0 ? null : roleColorCss(role.color);
                        return (
                          <span
                            key={role.role_id}
                            className="flex items-center gap-1 rounded-xs bg-rail px-2 py-0.5 text-xs text-norm"
                          >
                            <span
                              aria-hidden
                              className="h-2 w-2 rounded-full"
                              style={{
                                backgroundColor: couleur ?? 'rgb(var(--color-faint))',
                              }}
                            />
                            {role.name}
                          </span>
                        );
                      })}
                    </div>
                  </>
                )}

                {canNote && (
                  <>
                    <div className="mt-3 h-px bg-input/60" role="separator" />
                    <label
                      htmlFor="profil-note"
                      className="mt-2 block text-xs font-medium uppercase tracking-wide text-faint"
                    >
                      {t.profil.noteLabel}
                    </label>
                    <textarea
                      id="profil-note"
                      value={note}
                      onChange={(e) => setNote(e.target.value)}
                      onBlur={enregistrerNote}
                      maxLength={4096}
                      rows={2}
                      placeholder={t.profil.notePlaceholder}
                      className="mt-1 w-full resize-none rounded-md border border-transparent bg-input px-2 py-1.5 text-sm text-norm placeholder:text-faint outline-none transition-colors duration-fast focus:border-blurple/50"
                    />
                  </>
                )}

                {isSelf && (
                  <button
                    type="button"
                    onClick={modifierProfil}
                    className="mt-3 w-full rounded-sm bg-blurple px-3 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-[0.98]"
                  >
                    {t.profil.editProfile}
                  </button>
                )}

                {(canMessage || canRemove || canBlock) && !confirmRemove && (
                  <div className="mt-3 flex items-center gap-1.5">
                    {canMessage && (
                      <button
                        type="button"
                        onClick={envoyerMessage}
                        className="flex flex-1 items-center justify-center gap-1.5 rounded-full bg-blurple px-3 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-[0.97]"
                      >
                        <span
                          aria-hidden
                          className="flex h-4 w-4 shrink-0 items-center justify-center"
                        >
                          <EnvelopeMenuIcon size={16} />
                        </span>
                        {t.friends.sendDm}
                      </button>
                    )}
                    {canRemove && (
                      <button
                        type="button"
                        title={t.friends.remove}
                        aria-label={t.friends.remove}
                        onClick={() => setConfirmRemove(true)}
                        className="inline-flex shrink-0 items-center justify-center rounded-full p-2 text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
                      >
                        <RemoveFriendIcon />
                      </button>
                    )}
                    {canBlock && (
                      <button
                        type="button"
                        title={t.friends.block}
                        aria-label={t.friends.block}
                        onClick={bloquer}
                        className="inline-flex shrink-0 items-center justify-center rounded-full p-2 text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
                      >
                        <BlockUserIcon />
                      </button>
                    )}
                  </div>
                )}

                {confirmRemove && (
                  <div className="mt-3 rounded-lg border border-rail bg-rail/40 p-2.5">
                    <p className="text-sm text-norm">{t.friends.removeQuestion}</p>
                    <p className="mt-0.5 text-xs text-faint">
                      {t.friends.removeKeepHistory}
                    </p>
                    <div className="mt-2 flex gap-2">
                      <button
                        type="button"
                        onClick={retirer}
                        className="flex-1 rounded-full bg-red px-3 py-1.5 text-sm font-medium text-on-red transition-colors hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
                      >
                        {t.friends.remove}
                      </button>
                      <button
                        type="button"
                        onClick={() => setConfirmRemove(false)}
                        className="rounded-full bg-rail px-3 py-1.5 text-sm font-medium text-norm transition-colors hover:bg-input focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
                      >
                        {t.app.cancel}
                      </button>
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
