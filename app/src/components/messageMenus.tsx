/**
 * Items des menus contextuels du fil de messages (clic droit) : menu
 * « utilisateur » (profil, mention, MP, copie d'identifiant) et menu
 * « message » (copie, mention de l'auteur, réponse/transfert/épingle,
 * édition, suppression). Fonctions pures : tout le contexte du fil
 * (traductions, identité, actions câblées) arrive via `MessageMenuDeps`,
 * fourni par `MessageList`.
 */

import { interpolate, type Dict } from '../i18n';
import type { Contact, GroupThread } from '../lib/api';
import { useCalls } from '../stores/calls';
import type { ContextMenuItem } from '../stores/contextMenu';
import { useFriends } from '../stores/friends';
import { useUi } from '../stores/ui';
import {
  CopyMenuIcon,
  DeleteMenuIcon,
  EditMenuIcon,
  EnvelopeMenuIcon,
  ForwardMenuIcon,
  MentionMenuIcon,
  PhoneIcon,
  PinMenuIcon,
  ProfileMenuIcon,
  ReplyMenuIcon,
  SelectMenuIcon,
  ThreadMenuIcon,
} from './ContextMenu';
import {
  displayText,
  type DisplayMessage,
  type MessageListActions,
} from './messageModel';

/** Contexte du fil nécessaire à la construction des items de menu. */
export interface MessageMenuDeps {
  t: Dict;
  /** Pubkey de l'identité locale (null hors session). */
  selfPubkey: string | null;
  contacts: Contact[];
  /** Actions de message câblées par la vue ; absentes = fil en lecture seule. */
  actions: MessageListActions | undefined;
  nameOf: (author: string) => string;
  copyWithToast: (value: string, successText: string) => void;
  requestMentionInsert: (name: string) => void;
  /** Ouvre la carte de profil, ancrée sur l'élément cliqué. */
  openProfile: (author: string, target: HTMLElement) => void;
  /** Ouvre le sélecteur de transfert pour ce message. */
  onForward: (message: DisplayMessage) => void;
  /** Passe le message en édition en place. */
  onEditInPlace: (msgId: string) => void;
  /**
   * Fils du salon courant (`channelThreads`) : sert à distinguer « Créer un
   * fil » de « Ouvrir le fil » sur un message. Absent en MP (pas de fils).
   */
  threads?: readonly GroupThread[] | undefined;
  /**
   * Ouvre (ou crée puis ouvre) le fil ancré sur ce message — câblé par la vue
   * groupe. Absent = pas d'entrée « fil » (MP, ou fil lui-même).
   */
  onOpenThread?: ((message: DisplayMessage) => void) | undefined;
  /**
   * Active le mode sélection de messages (suppression groupée) en pré-cochant
   * ce message — câblé par la vue groupe pour un porteur de `MANAGE_MESSAGES`.
   * Absent = pas d'entrée « Sélectionner des messages » (MP, non-modérateur).
   */
  onStartSelection?: ((message: DisplayMessage) => void) | undefined;
}

/** Icône « retirer l'ami » du jeu de menu (14 px) — personne barrée d'un moins. */
function RemoveFriendMenuIcon() {
  return (
    <svg
      width={14}
      height={14}
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

/** Icône « bloquer/débloquer » du jeu de menu (14 px) — cercle barré. */
function BlockUserMenuIcon() {
  return (
    <svg
      width={14}
      height={14}
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

/**
 * Items du menu contextuel « utilisateur » (avatar, pseudo d'un message) :
 * profil, mention, MP et appel (amis confirmés), puis les actions de relation
 * (retrait d'amitié, blocage/déblocage) et la copie d'identifiant — chacune
 * réutilisant une action déjà existante des stores de domaine (`ui.setView`,
 * `calls.start`, `friends.remove/block/unblock`), à l'image de la carte de
 * profil. Les actions d'administration serveur (pseudo, rôles, exclusion,
 * expulsion, bannissement) vivent dans le menu de la liste des membres
 * (`ChatView`), qui dispose du contexte de groupe requis — ce menu-ci n'a que
 * l'identité du pair.
 */
export function buildUserItems(
  deps: MessageMenuDeps,
  author: string,
  target: HTMLElement,
): ContextMenuItem[] {
  const { t, selfPubkey, contacts, nameOf, copyWithToast, requestMentionInsert } = deps;
  const isSelfAuthor = selfPubkey !== null && author === selfPubkey;
  const contact = contacts.find((c) => c.pubkey === author);
  const isFriend = contact?.state === 'friend';
  const isBlocked = contact?.state === 'blocked';
  const canMessage = !isSelfAuthor && isFriend;
  const onActionError = (): void =>
    useUi.getState().toast('error', t.errors.actionFailed);
  const items: ContextMenuItem[] = [
    {
      label: t.contextMenu.viewProfile,
      icon: <ProfileMenuIcon />,
      onClick: () => deps.openProfile(author, target),
    },
    {
      label: interpolate(t.contextMenu.mention, { name: nameOf(author) }),
      icon: <MentionMenuIcon />,
      onClick: () => requestMentionInsert(nameOf(author)),
    },
  ];
  if (canMessage) {
    items.push({
      label: t.friends.sendDm,
      icon: <EnvelopeMenuIcon />,
      onClick: () => useUi.getState().setView({ kind: 'dm', peer: author }),
    });
    // Appel 1-à-1 : réservé aux amis confirmés côté nœud (contrat calls.start),
    // même garde que le bouton d'appel de l'en-tête MP (`ChatView`).
    items.push({
      label: t.calls.startCall,
      icon: <PhoneIcon size={14} />,
      onClick: () => {
        useCalls.getState().start(author).catch(onActionError);
      },
    });
  }
  // Relation d'amitié : retrait (confirmé, l'historique est conservé) et
  // blocage/déblocage — mêmes actions que la carte de profil (`ProfilePopover`).
  if (isFriend) {
    items.push({
      label: t.friends.remove,
      icon: <RemoveFriendMenuIcon />,
      danger: true,
      separatorBefore: true,
      onClick: () => {
        if (!window.confirm(t.friends.removeQuestion)) return;
        useFriends.getState().remove(author).catch(onActionError);
      },
    });
  }
  if (isBlocked) {
    items.push({
      label: t.friends.unblock,
      icon: <BlockUserMenuIcon />,
      separatorBefore: !isFriend,
      onClick: () => {
        useFriends.getState().unblock(author).catch(onActionError);
      },
    });
  } else if (!isSelfAuthor) {
    items.push({
      label: t.friends.block,
      icon: <BlockUserMenuIcon />,
      danger: true,
      separatorBefore: !isFriend,
      onClick: () => {
        useFriends.getState().block(author).catch(onActionError);
      },
    });
  }
  items.push({
    label: t.contextMenu.copyUserId,
    icon: <CopyMenuIcon />,
    separatorBefore: true,
    onClick: () => copyWithToast(author, t.app.copied),
  });
  return items;
}

/**
 * Items du menu contextuel « message » : copie, mention de l'auteur, puis
 * réponse/transfert/épingle/édition/suppression — chacun réutilisant une
 * action déjà câblée par `actions` (ou omis si l'action n'existe pas ou
 * n'est pas permise), à l'image de la barre d'actions au survol.
 */
export function buildMessageItems(
  deps: MessageMenuDeps,
  message: DisplayMessage,
  isOwn: boolean,
  pinned: boolean,
): ContextMenuItem[] {
  const { t, actions, nameOf, copyWithToast, requestMentionInsert } = deps;
  const text = !message.deleted ? displayText(message) : null;
  // Un sticker n'a pas de texte à éditer (contrat : pas de fusion
  // texte/sticker) — seule la suppression reste permise.
  const canEdit =
    actions !== undefined && !message.deleted && isOwn && message.body.type === 'text';
  const canDelete =
    actions !== undefined &&
    !message.deleted &&
    (isOwn || actions.canModerate === true);
  const items: ContextMenuItem[] = [];
  if (text !== null && text !== '') {
    items.push({
      label: t.contextMenu.copyText,
      icon: <CopyMenuIcon />,
      onClick: () => copyWithToast(text, t.app.copied),
    });
  }
  items.push({
    label: t.contextMenu.copyMessageId,
    icon: <CopyMenuIcon />,
    onClick: () => copyWithToast(message.msg_id, t.app.copied),
  });
  if (actions !== undefined && !message.deleted) {
    items.push({
      label: interpolate(t.contextMenu.mention, { name: nameOf(message.author) }),
      icon: <MentionMenuIcon />,
      separatorBefore: true,
      onClick: () => requestMentionInsert(nameOf(message.author)),
    });
  }
  const flow: ContextMenuItem[] = [];
  if (!message.deleted && actions?.onReply !== undefined) {
    flow.push({
      label: t.dm.reply,
      icon: <ReplyMenuIcon />,
      onClick: () => actions.onReply?.(message),
    });
  }
  if (!message.deleted && actions !== undefined) {
    flow.push({
      label: t.dm.forward,
      icon: <ForwardMenuIcon />,
      onClick: () => deps.onForward(message),
    });
  }
  if (!message.deleted && actions?.onTogglePin !== undefined) {
    flow.push({
      label: pinned ? t.serveur.unpin : t.serveur.pin,
      icon: <PinMenuIcon />,
      onClick: () => actions.onTogglePin?.(message, pinned),
    });
  }
  // Fil de discussion (salons seulement) : « Ouvrir » si un fil est déjà ancré
  // sur ce message, sinon « Créer ». La décision création/ouverture réelle vit
  // dans la vue (`onOpenThread`) ; le libellé n'a besoin que de la présence.
  if (!message.deleted && deps.onOpenThread !== undefined) {
    const hasThread = (deps.threads ?? []).some((th) => th.root_msg === message.msg_id);
    flow.push({
      label: hasThread ? t.threads.openThread : t.threads.createThread,
      icon: <ThreadMenuIcon />,
      onClick: () => deps.onOpenThread?.(message),
    });
  }
  flow.forEach((item, i) =>
    items.push(i === 0 ? { ...item, separatorBefore: true } : item),
  );

  const management: ContextMenuItem[] = [];
  // Modération : entrée en mode sélection (suppression groupée). Disponible dès
  // que la vue la câble (porteur de MANAGE_MESSAGES), y compris sur un message
  // d'autrui, et indépendante de la garde suppression auteur-seul ci-dessous.
  if (deps.onStartSelection !== undefined && !message.deleted) {
    management.push({
      label: t.purge.select,
      icon: <SelectMenuIcon />,
      onClick: () => deps.onStartSelection?.(message),
    });
  }
  if (canEdit) {
    management.push({
      label: t.dm.edit,
      icon: <EditMenuIcon />,
      onClick: () => deps.onEditInPlace(message.msg_id),
    });
  }
  if (canDelete) {
    management.push({
      label: t.dm.delete,
      icon: <DeleteMenuIcon />,
      danger: true,
      onClick: () => actions?.onDelete(message),
    });
  }
  management.forEach((item, i) =>
    items.push(i === 0 ? { ...item, separatorBefore: true } : item),
  );

  return items;
}
