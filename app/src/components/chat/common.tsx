/**
 * Briques partagées des vues de conversation (MP et salon), extraites de
 * `ChatView` (D-056) : saut vers un message, résolution des épinglés,
 * mentions connues, nom de fil par défaut, bouton d'en-tête et bandeau de
 * réponse. Comportement inchangé.
 */

import { useEffect, useState } from 'react';
import type { Contact, SelfProfile } from '../../lib/api';
import { selfDisplayName } from '../../stores/session';
import { useUi, useT, type JumpRequest } from '../../stores/ui';
import { CloseIcon } from '../ContextMenu';
import type { DisplayMessage } from '../MessageList';

/**
 * Traite la demande de saut de l'UI qui vise la vue courante : charge la
 * fenêtre du message (via `load`) puis rend la cible à révéler (défilement +
 * surbrillance dans `MessageList`). Une cible absente déclenche un toast.
 */
export function useMessageJump(
  matches: (jump: JumpRequest) => boolean,
  load: (msgId: string) => Promise<boolean>,
): { msgId: string; nonce: number } | null {
  const jump = useUi((s) => s.jump);
  const clearJump = useUi((s) => s.clearJump);
  const toast = useUi((s) => s.toast);
  const t = useT();
  const [scrollTarget, setScrollTarget] = useState<{
    msgId: string;
    nonce: number;
  } | null>(null);

  useEffect(() => {
    if (jump === null || !matches(jump)) return;
    const req = jump;
    let cancelled = false;
    void (async () => {
      let found = false;
      try {
        found = await load(req.msgId);
      } catch {
        found = false;
      }
      if (cancelled) return;
      if (found) setScrollTarget({ msgId: req.msgId, nonce: req.nonce });
      else toast('error', t.dm.messageUnavailable);
      clearJump();
    })();
    return () => {
      cancelled = true;
    };
    // `matches`/`load` capturent la vue courante ; seul un nouveau saut relance.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [jump]);

  return scrollTarget;
}

/** Messages épinglés résolus dans l'historique chargé + nombre hors-page. */
export function resolvePins(
  pinnedIds: readonly string[],
  messages: readonly DisplayMessage[],
): { resolved: DisplayMessage[]; unresolved: number } {
  const byId = new Map(messages.map((m) => [m.msg_id, m]));
  const resolved = pinnedIds.flatMap((id) => {
    const message = byId.get(id);
    return message !== undefined && !message.deleted ? [message] : [];
  });
  return { resolved, unresolved: pinnedIds.length - resolved.length };
}

/** Noms (en minuscules) reconnus comme mentions : contacts nommés + soi-même. */
export function mentionSet(contacts: Contact[], self: SelfProfile | null): Set<string> {
  const noms = new Set<string>();
  for (const c of contacts) {
    if (c.display_name.trim() !== '') noms.add(c.display_name.toLowerCase());
  }
  if (self !== null) noms.add(selfDisplayName(self).toLowerCase());
  return noms;
}

/** Longueur maximale d'un nom de fil dérivé du texte du message racine. */
const THREAD_NAME_MAX = 50;

/**
 * Nom de fil par défaut : début de la première ligne du message racine (tronqué),
 * ou `fallback` si le message n'a pas de texte exploitable.
 */
export function deriveThreadName(text: string, fallback: string): string {
  const firstLine = text.split('\n')[0]?.trim() ?? '';
  if (firstLine === '') return fallback;
  return firstLine.length > THREAD_NAME_MAX
    ? `${firstLine.slice(0, THREAD_NAME_MAX).trimEnd()}…`
    : firstLine;
}

/** Bouton d'action de l'en-tête de conversation : conteneur carré fixe (icon spec). */
export function HeaderIconButton({
  label,
  active,
  onClick,
  ariaExpanded,
  buttonRef,
  disabled = false,
  children,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  ariaExpanded?: boolean;
  buttonRef?: React.Ref<HTMLButtonElement>;
  disabled?: boolean;
  children: React.ReactNode;
}) {
  return (
    <button
      ref={buttonRef}
      type="button"
      aria-label={label}
      title={label}
      aria-expanded={ariaExpanded}
      disabled={disabled}
      onClick={onClick}
      className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-md transition-colors duration-fast hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat active:scale-95 disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:bg-transparent ${
        active ? 'text-header' : 'text-muted hover:text-norm'
      }`}
    >
      {children}
    </button>
  );
}

/** Bandeau « Répondre à … » au-dessus de la zone de saisie, annulable. */
export function ReplyBanner({ name, onCancel }: { name: string; onCancel: () => void }) {
  const t = useT();
  // Le nom est mis en gras quelle que soit sa position dans le libellé.
  const [before, after] = t.dm.replyingTo.split('{name}');
  return (
    <div className="relative z-[1] mx-4 -mb-1 flex items-center justify-between gap-2 rounded-t-xl border border-b-0 border-rail/60 bg-sidebar px-4 py-2 text-sm">
      <span className="min-w-0 truncate text-muted">
        {before}
        <span className="font-semibold text-header">{name}</span>
        {after}
      </span>
      <button
        type="button"
        aria-label={t.dm.cancelReply}
        title={t.dm.cancelReply}
        onClick={onCancel}
        // 22 → 38 px, en débordant dans le `px-4 py-2` du bandeau. Rien ne
        // bouge à l'écran ; le bandeau porte déjà `z-[1]`, donc les quelques
        // pixels qui dépassent restent au-dessus du composeur.
        className="relative -m-2 flex shrink-0 items-center justify-center rounded-full p-3 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar active:scale-90"
      >
        <CloseIcon size={14} />
      </button>
    </div>
  );
}
