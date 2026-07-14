/**
 * Barre d'actions flottante d'un message (survol), à la Discord :
 * réaction rapide (petit choix d'emojis courants), réponse (MP uniquement),
 * épinglage (salons, avec MANAGE_MESSAGES), puis édition (auteur seul) et
 * suppression (auteur, ou modération). La suppression demande une
 * confirmation légère en place ; Échap referme les volets ouverts.
 */

import { useState } from 'react';
import { interpolate } from '../i18n';
import { valeurReaction } from '../lib/emoji';
import { useT } from '../stores/ui';
import { EmojiPicker } from './EmojiPicker';

/** Choix restreint d'emojis courants proposés au survol. */
export const QUICK_EMOJIS = ['👍', '❤️', '😂', '😮', '😢', '🎉'] as const;

interface MessageActionsProps {
  /** Édition permise (auteur du message uniquement). */
  canEdit: boolean;
  /** Suppression permise (auteur, ou MANAGE_MESSAGES en salon). */
  canDelete: boolean;
  onReact: (emoji: string) => void;
  /** Réponse citée — absente dans les salons (non prévue par l'API). */
  onReply?: (() => void) | undefined;
  onEdit: () => void;
  onDelete: () => void;
  /** Transfert du message vers une autre conversation (MP et salons). */
  onForward?: (() => void) | undefined;
  /** Copie d'un lien `accord:` vers le message dans le presse-papiers. */
  onCopyLink?: (() => void) | undefined;
  /** Épinglage — absent hors salon ou sans MANAGE_MESSAGES. */
  onTogglePin?: (() => void) | undefined;
  /** Vrai si le message est épinglé (le libellé devient « Désépingler »). */
  pinned?: boolean;
  /** Contexte serveur : expose ses émojis custom au sélecteur (`null` en MP). */
  groupId?: string | null;
}

function ActionButton({
  label,
  danger = false,
  onClick,
  children,
}: {
  label: string;
  danger?: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      onClick={onClick}
      className={`flex h-8 w-8 items-center justify-center transition-colors hover:bg-chat-hover focus-visible:bg-chat-hover focus-visible:outline-none ${
        danger
          ? 'text-muted hover:text-red focus-visible:text-red'
          : 'text-muted hover:text-norm focus-visible:text-norm'
      }`}
    >
      {children}
    </button>
  );
}

export function MessageActions({
  canEdit,
  canDelete,
  onReact,
  onReply,
  onEdit,
  onDelete,
  onForward,
  onCopyLink,
  onTogglePin,
  pinned = false,
  groupId = null,
}: MessageActionsProps) {
  const t = useT();
  const [pickerOpen, setPickerOpen] = useState(false);
  const [moreOpen, setMoreOpen] = useState(false);
  const [confirming, setConfirming] = useState(false);

  const closePanels = (): void => {
    setPickerOpen(false);
    setMoreOpen(false);
    setConfirming(false);
  };

  // Forcée visible quand un volet est ouvert, sinon révélée au survol/focus.
  const revealed = pickerOpen || moreOpen || confirming;

  return (
    <div
      // La barre reste montée en permanence (révélation CSS par group-hover) :
      // `.popover-enter` seule ne jouerait qu'au montage. On déclenche donc les
      // mêmes keyframes/tokens (`scale-fade-in`, durées/courbes de global.css)
      // sur les variantes de survol/focus — hors état `revealed`, pour ne pas
      // rejouer l'animation sur une barre déjà visible (volet ouvert).
      className={`absolute -top-4 right-4 z-10 focus-within:pointer-events-auto focus-within:opacity-100 group-hover:pointer-events-auto group-hover:opacity-100 ${
        revealed
          ? 'pointer-events-auto opacity-100'
          : 'pointer-events-none opacity-0 focus-within:animate-[scale-fade-in_var(--duration-fast)_var(--ease-out)] group-hover:animate-[scale-fade-in_var(--duration-fast)_var(--ease-out)]'
      }`}
      onKeyDown={(e) => {
        if (e.key === 'Escape') closePanels();
      }}
    >
      {pickerOpen && (
        <div
          role="menu"
          aria-label={t.dm.addReaction}
          className="glass-strong popover-enter absolute bottom-full right-0 mb-1.5 flex gap-0.5 rounded-lg p-1"
        >
          {QUICK_EMOJIS.map((emoji) => (
            <button
              key={emoji}
              type="button"
              role="menuitem"
              aria-label={interpolate(t.dm.reactWith, { emoji })}
              title={interpolate(t.dm.reactWith, { emoji })}
              onClick={() => {
                setPickerOpen(false);
                onReact(emoji);
              }}
              className="rounded-full p-1 text-lg leading-none transition-transform hover:scale-125 hover:bg-chat-hover focus-visible:scale-125 focus-visible:outline-none"
            >
              {emoji}
            </button>
          ))}
          <button
            type="button"
            role="menuitem"
            aria-label={t.emoji.more}
            title={t.emoji.more}
            onClick={() => {
              setPickerOpen(false);
              setMoreOpen(true);
            }}
            className="rounded-full p-1 text-muted transition-colors hover:bg-chat-hover hover:text-norm focus-visible:bg-chat-hover focus-visible:text-norm focus-visible:outline-none"
          >
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M5 12h14" />
              <path d="M12 5v14" />
            </svg>
          </button>
        </div>
      )}
      {moreOpen && (
        <EmojiPicker
          groupId={groupId}
          positionClass="bottom-full right-0 mb-1.5"
          onSelect={(pick) => {
            setMoreOpen(false);
            onReact(valeurReaction(pick));
          }}
          onClose={() => setMoreOpen(false)}
        />
      )}
      {confirming && (
        <div
          role="alertdialog"
          aria-label={t.dm.deleteConfirm}
          className="glass-strong popover-enter absolute bottom-full right-0 mb-1.5 flex items-center gap-2 whitespace-nowrap rounded-lg px-3 py-2"
        >
          <span className="text-sm text-norm">{t.dm.deleteConfirm}</span>
          <button
            type="button"
            onClick={() => {
              setConfirming(false);
              onDelete();
            }}
            className="rounded-sm bg-red px-2.5 py-1 text-xs font-medium text-on-red transition-colors hover:bg-red/80 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-red focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
          >
            {t.dm.delete}
          </button>
          <button
            type="button"
            onClick={() => setConfirming(false)}
            className="rounded-sm px-1 py-1 text-xs font-medium text-norm hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
          >
            {t.app.cancel}
          </button>
        </div>
      )}
      <div className="glass-strong flex items-center overflow-hidden rounded-lg">
        <ActionButton
          label={t.dm.addReaction}
          onClick={() => {
            setConfirming(false);
            setPickerOpen((open) => !open);
          }}
        >
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden
          >
            <circle cx="12" cy="12" r="10" />
            <path d="M8 14s1.5 2 4 2 4-2 4-2" />
            <line x1="9" x2="9.01" y1="9" y2="9" />
            <line x1="15" x2="15.01" y1="9" y2="9" />
          </svg>
        </ActionButton>
        {onReply !== undefined && (
          <ActionButton
            label={t.dm.reply}
            onClick={() => {
              closePanels();
              onReply();
            }}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <polyline points="9 17 4 12 9 7" />
              <path d="M20 18v-2a4 4 0 0 0-4-4H4" />
            </svg>
          </ActionButton>
        )}
        {onForward !== undefined && (
          <ActionButton
            label={t.dm.forward}
            onClick={() => {
              closePanels();
              onForward();
            }}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <polyline points="15 17 20 12 15 7" />
              <path d="M4 18v-2a4 4 0 0 1 4-4h12" />
            </svg>
          </ActionButton>
        )}
        {onCopyLink !== undefined && (
          <ActionButton
            label={t.dm.copyLink}
            onClick={() => {
              closePanels();
              onCopyLink();
            }}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M9 17H7A5 5 0 0 1 7 7h2" />
              <path d="M15 7h2a5 5 0 1 1 0 10h-2" />
              <line x1="8" x2="16" y1="12" y2="12" />
            </svg>
          </ActionButton>
        )}
        {onTogglePin !== undefined && (
          <ActionButton
            label={pinned ? t.serveur.unpin : t.serveur.pin}
            onClick={() => {
              closePanels();
              onTogglePin();
            }}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <line x1="12" x2="12" y1="17" y2="22" />
              <path d="M5 17h14v-1.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V6h1a2 2 0 0 0 0-4H8a2 2 0 0 0 0 4h1v4.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24Z" />
            </svg>
          </ActionButton>
        )}
        {canEdit && (
          <ActionButton
            label={t.dm.edit}
            onClick={() => {
              closePanels();
              onEdit();
            }}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M17 3a2.85 2.83 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5Z" />
              <path d="m15 5 4 4" />
            </svg>
          </ActionButton>
        )}
        {canDelete && (
          <ActionButton
            label={t.dm.delete}
            danger
            onClick={() => {
              setPickerOpen(false);
              setConfirming((open) => !open);
            }}
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M3 6h18" />
              <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
              <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
              <line x1="10" x2="10" y1="11" y2="17" />
              <line x1="14" x2="14" y1="11" y2="17" />
            </svg>
          </ActionButton>
        )}
      </div>
    </div>
  );
}
