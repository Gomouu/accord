/**
 * Menu contextuel « contact » (clic droit sur une conversation privée de la
 * barre latérale d'accueil ou une ligne de la vue Amis) : profil, message,
 * appel, marquer comme lu, demandes en attente, copie du code ami, retrait et
 * blocage. Fonction pure réutilisant les actions déjà câblées des stores de
 * domaine (`ui`, `calls`, `dms`, `friends`) — à l'image du menu utilisateur du
 * fil (`messageMenus`) et de la carte de profil, sans dupliquer leur logique.
 */

import type { Dict } from '../i18n';
import type { Contact, OwnPresenceStatus, SelfProfile } from '../lib/api';
import { copyToClipboard } from '../lib/clipboard';
import { markDmRead } from '../lib/markRead';
import { useCalls } from '../stores/calls';
import type { ContextMenuItem } from '../stores/contextMenu';
import { useFriends } from '../stores/friends';
import { usePinnedDms } from '../stores/pinnedDms';
import { useUi, type AncrePopover } from '../stores/ui';
import {
  CheckMenuIcon,
  CopyMenuIcon,
  DeleteMenuIcon,
  EnvelopeMenuIcon,
  GearMenuIcon,
  PhoneIcon,
  PinMenuIcon,
  ProfileMenuIcon,
} from './ContextMenu';
import { BlockUserMenuIcon, RemoveFriendMenuIcon } from './messageMenus';

/** Ancre de carte de profil dérivée de l'élément (ligne) ayant reçu le clic droit. */
function ancreDe(target: HTMLElement): AncrePopover {
  const r = target.getBoundingClientRect();
  return { top: r.top, left: r.left, bottom: r.bottom, right: r.right };
}

/** Items du menu contextuel d'un contact (`target` = la ligne cliquée). */
export function buildContactMenu(
  t: Dict,
  contact: Contact,
  target: HTMLElement,
): ContextMenuItem[] {
  const onError = (): void => useUi.getState().toast('error', t.errors.actionFailed);
  const copie = (valeur: string): void =>
    copyToClipboard(
      valeur,
      () => useUi.getState().toast('success', t.app.copied),
      onError,
    );
  const isFriend = contact.state === 'friend';
  const isBlocked = contact.state === 'blocked';
  const hasUnread = (contact.unread ?? 0) > 0;

  const items: ContextMenuItem[] = [
    {
      label: t.contextMenu.viewProfile,
      icon: <ProfileMenuIcon />,
      onClick: () => useUi.getState().openProfile(contact.pubkey, ancreDe(target), null),
    },
  ];

  if (isFriend) {
    items.push({
      label: t.friends.sendDm,
      icon: <EnvelopeMenuIcon />,
      onClick: () => useUi.getState().setView({ kind: 'dm', peer: contact.pubkey }),
    });
    items.push({
      label: t.calls.startCall,
      icon: <PhoneIcon size={14} />,
      onClick: () => {
        useCalls.getState().start(contact.pubkey).catch(onError);
      },
    });
    if (hasUnread) {
      items.push({
        label: t.contextMenu.markAsRead,
        icon: <CheckMenuIcon />,
        onClick: () => void markDmRead(contact.pubkey).catch(onError),
      });
    }
    const pinned = usePinnedDms.getState().isPinned(contact.pubkey);
    items.push({
      label: pinned ? t.contextMenu.unpinConversation : t.contextMenu.pinConversation,
      icon: <PinMenuIcon />,
      onClick: () => usePinnedDms.getState().toggle(contact.pubkey),
    });
  }

  // Demande d'ami reçue : mêmes actions que les boutons en ligne de la vue Amis.
  if (contact.state === 'pending_in') {
    items.push({
      label: t.friends.accept,
      icon: <CheckMenuIcon />,
      onClick: () => {
        useFriends.getState().respond(contact.pubkey, true).catch(onError);
      },
    });
    items.push({
      label: t.friends.decline,
      icon: <DeleteMenuIcon />,
      onClick: () => {
        useFriends.getState().respond(contact.pubkey, false).catch(onError);
      },
    });
  }

  items.push({
    label: t.contextMenu.copyFriendCode,
    icon: <CopyMenuIcon />,
    separatorBefore: true,
    onClick: () => copie(contact.friend_code),
  });

  if (isFriend) {
    items.push({
      label: t.friends.remove,
      icon: <RemoveFriendMenuIcon />,
      danger: true,
      separatorBefore: true,
      onClick: () => {
        if (!window.confirm(t.friends.removeQuestion)) return;
        useFriends.getState().remove(contact.pubkey).catch(onError);
      },
    });
  }

  if (isBlocked) {
    items.push({
      label: t.friends.unblock,
      icon: <BlockUserMenuIcon />,
      separatorBefore: !isFriend,
      onClick: () => {
        useFriends.getState().unblock(contact.pubkey).catch(onError);
      },
    });
  } else {
    items.push({
      label: t.friends.block,
      icon: <BlockUserMenuIcon />,
      danger: true,
      separatorBefore: !isFriend,
      onClick: () => {
        useFriends.getState().block(contact.pubkey).catch(onError);
      },
    });
  }

  return items;
}

/**
 * Items du menu contextuel de l'utilisateur local (clic droit sur son propre
 * panneau, en bas de la barre latérale) : choix du statut de présence (radios),
 * copie du code ami et de l'ID, accès aux paramètres — mêmes actions que le
 * menu utilisateur ouvert au clic (`UserMenu`), en accès direct.
 */
export function buildOwnUserMenu(
  t: Dict,
  self: SelfProfile,
  ownStatus: OwnPresenceStatus,
): ContextMenuItem[] {
  const onError = (): void => useUi.getState().toast('error', t.errors.actionFailed);
  const copie = (valeur: string): void =>
    copyToClipboard(
      valeur,
      () => useUi.getState().toast('success', t.app.copied),
      onError,
    );
  const statut = (etat: OwnPresenceStatus): ContextMenuItem => ({
    label: t.profil[etat],
    checked: ownStatus === etat,
    onClick: () => {
      useFriends.getState().setOwnStatus(etat).catch(onError);
    },
  });

  return [
    statut('online'),
    statut('idle'),
    statut('dnd'),
    statut('invisible'),
    {
      label: t.profil.copyFriendCode,
      icon: <CopyMenuIcon />,
      separatorBefore: true,
      onClick: () => copie(self.friend_code),
    },
    {
      label: t.contextMenu.copyUserId,
      icon: <CopyMenuIcon />,
      onClick: () => copie(self.pubkey),
    },
    {
      label: t.settings.title,
      icon: <GearMenuIcon />,
      separatorBefore: true,
      onClick: () => useUi.getState().openModal({ kind: 'settings' }),
    },
  ];
}
