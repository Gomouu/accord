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
import type { ContextMenuItem } from '../stores/contextMenu';
import { useUi } from '../stores/ui';
import {
  CopyMenuIcon,
  DeleteMenuIcon,
  EditMenuIcon,
  EnvelopeMenuIcon,
  ForwardMenuIcon,
  MentionMenuIcon,
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

/**
 * Items du menu contextuel « utilisateur » (avatar, pseudo, entrée de la
 * liste des membres) : profil, mention, MP, copie d'identifiant — réutilise
 * l'ouverture de profil, le pont de mention (`requestMentionInsert`) et
 * l'action d'ouverture de MP déjà existante (`ui.setView`).
 */
export function buildUserItems(
  deps: MessageMenuDeps,
  author: string,
  target: HTMLElement,
): ContextMenuItem[] {
  const { t, selfPubkey, contacts, nameOf, copyWithToast, requestMentionInsert } = deps;
  const isSelfAuthor = selfPubkey !== null && author === selfPubkey;
  const contact = contacts.find((c) => c.pubkey === author);
  const canMessage = !isSelfAuthor && contact?.state === 'friend';
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
