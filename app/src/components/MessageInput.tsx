/**
 * Zone de saisie : Entrée envoie, Maj+Entrée insère un saut de ligne.
 * Pièces jointes : bouton trombone, glisser-déposer sur la zone de saisie,
 * collage d'image — aperçus retirables, bornes UI (10 pièces, 8 Mio chacune).
 * À l'envoi, chaque pièce est publiée via `files.share_bytes` (état
 * « publication… »), puis le message part avec ses références.
 */

import { useMemo, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { FileAttachment } from '../lib/api';
import {
  estImage,
  fichierEnB64,
  fichierEnDataUrl,
  validerAjout,
} from '../lib/attachments';
import { jetonTexteEmoji, type EmojiPick } from '../lib/emoji';
import {
  findActiveMention,
  filterMentions,
  groupMentionCandidates,
  insertMention,
  type ActiveMention,
  type MentionCandidate,
} from '../lib/mentions';
import { api } from '../lib/client';
import { formatTimestamp, tailleLisible } from '../lib/format';
import { useTypingEmitter, type TypingTarget } from '../hooks/useTypingEmitter';
import { useFriends, displayNameOf } from '../stores/friends';
import {
  isChannelReadOnly,
  sortRoles,
  timeoutUntil,
  useGroups,
} from '../stores/groups';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { EmojiPicker } from './EmojiPicker';
import { MentionAutocomplete } from './MentionAutocomplete';

/** Pièce en attente d'envoi (aperçu local, avant publication). */
interface PieceEnAttente {
  id: number;
  file: File;
  /** Aperçu image en URL `data:` (chargé après lecture), `null` sinon. */
  url: string | null;
}

interface MessageInputProps {
  placeholder: string;
  onSend: (text: string, attachments?: FileAttachment[]) => Promise<void>;
  /** Contexte serveur : expose ses émojis custom au sélecteur (`null` en MP). */
  groupId?: string | null;
  /** Cible de l'indicateur de frappe (absente : aucune émission). */
  typingTarget?: TypingTarget | undefined;
}

let prochainId = 1;

export function MessageInput({
  placeholder,
  onSend,
  groupId = null,
  typingTarget,
}: MessageInputProps) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  /** Signale la frappe au pair/salon (throttlé, best effort). */
  const notifyTyping = useTypingEmitter(typingTarget);
  const [text, setText] = useState('');
  const [sending, setSending] = useState(false);
  const [pieces, setPieces] = useState<PieceEnAttente[]>([]);
  const [erreur, setErreur] = useState<string | null>(null);
  const [survol, setSurvol] = useState(false);
  const [emojiOpen, setEmojiOpen] = useState(false);
  const fileRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  /* @mention autocomplete: candidates from the group state (members, roles
   * and the two broadcasts); no candidates in a direct message. */
  const groupState = useGroups((s) => (groupId !== null ? s.states[groupId] : undefined));
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const candidates = useMemo<MentionCandidate[]>(() => {
    if (groupState === undefined) return [];
    const nameOf = (pubkey: string): string =>
      self !== null && pubkey === self.pubkey
        ? selfDisplayName(self)
        : displayNameOf(contacts, pubkey);
    return groupMentionCandidates(groupState.members, sortRoles(groupState.roles), nameOf);
  }, [groupState, contacts, self]);

  const [mention, setMention] = useState<ActiveMention | null>(null);
  const [mentionIndex, setMentionIndex] = useState(0);
  const suggestions =
    mention !== null ? filterMentions(candidates, mention.query) : [];
  const mentionOpen = suggestions.length > 0;
  const activeIndex = Math.min(mentionIndex, suggestions.length - 1);

  /** Recomputes the active mention from the caret without resetting the
   * highlighted index (used on caret moves; typing resets it separately). */
  const syncCaret = (el: HTMLTextAreaElement): void => {
    setMention(findActiveMention(el.value, el.selectionStart ?? el.value.length));
  };

  /** Inserts the chosen candidate and closes the popup. */
  const chooseMention = (candidate: MentionCandidate | undefined): void => {
    if (candidate === undefined || mention === null) return;
    const { text: next, caret } = insertMention(text, mention, candidate);
    setText(next);
    setMention(null);
    const el = textareaRef.current;
    if (el !== null) {
      requestAnimationFrame(() => {
        el.focus();
        el.setSelectionRange(caret, caret);
      });
    }
  };

  /** Insère le jeton d'un émoji choisi à la position du curseur. */
  const insererEmoji = (pick: EmojiPick): void => {
    const jeton = jetonTexteEmoji(pick);
    const el = textareaRef.current;
    if (el === null) {
      setText((prev) => prev + jeton);
      return;
    }
    const start = el.selectionStart ?? text.length;
    const end = el.selectionEnd ?? text.length;
    const suivant = text.slice(0, start) + jeton + text.slice(end);
    setText(suivant);
    requestAnimationFrame(() => {
      el.focus();
      const pos = start + jeton.length;
      el.setSelectionRange(pos, pos);
    });
  };

  /** Ajoute des fichiers en respectant les bornes (10 pièces, 8 Mio). */
  const ajouter = (fichiers: File[]): void => {
    if (fichiers.length === 0 || sending) return;
    const bilan = validerAjout(pieces.length, fichiers);
    if (bilan.refusesTaille.length > 0) {
      setErreur(
        interpolate(t.fichiers.tropVolumineux, { name: bilan.refusesTaille[0] ?? '' }),
      );
    } else if (bilan.refusesNombre > 0) {
      setErreur(t.fichiers.tropDeFichiers);
    } else {
      setErreur(null);
    }
    if (bilan.acceptes.length === 0) return;
    const nouvelles = bilan.acceptes.map((file) => ({
      id: prochainId++,
      file,
      url: null,
    }));
    setPieces((p) => [...p, ...nouvelles]);
    // Aperçus image en data: URL, chargés hors du rendu (blob: non rendue
    // par la WKWebView packagée). Une pièce retirée entre-temps est ignorée.
    for (const piece of nouvelles) {
      if (!estImage(piece.file.type)) continue;
      void fichierEnDataUrl(piece.file)
        .then((url) => {
          setPieces((p) => p.map((x) => (x.id === piece.id ? { ...x, url } : x)));
        })
        .catch(() => {
          // Fichier illisible : la pièce reste listée sans vignette.
        });
    }
  };

  const retirer = (id: number): void => {
    setPieces((p) => p.filter((x) => x.id !== id));
    setErreur(null);
  };

  const submit = async (): Promise<void> => {
    const trimmed = text.trim();
    if ((trimmed === '' && pieces.length === 0) || sending) return;
    setSending(true);
    try {
      // Publication séquentielle des pièces dans le magasin local.
      const attachments: FileAttachment[] = [];
      for (const piece of pieces) {
        const dataB64 = await fichierEnB64(piece.file);
        const { file } = await api.filesShareBytes(
          piece.file.name,
          piece.file.type !== '' ? piece.file.type : 'application/octet-stream',
          dataB64,
        );
        attachments.push(file);
      }
      await onSend(trimmed, attachments.length > 0 ? attachments : undefined);
      setPieces([]);
      setText('');
      setErreur(null);
    } catch {
      // Publication ou envoi refusé : l'utilisateur peut réessayer tel quel.
      setErreur(t.errors.sendFailed);
    } finally {
      setSending(false);
    }
  };

  const publierEnCours = sending && pieces.length > 0;

  // Composeur en lecture seule : sourdine active de l'utilisateur local, ou
  // salon d'annonces sans MANAGE_CHANNELS effectif. Le fil reste consultable.
  const channelId = typingTarget?.kind === 'group' ? typingTarget.channelId : null;
  const selfMember =
    self !== null && groupState !== undefined
      ? groupState.members.find((m) => m.pubkey === self.pubkey)
      : undefined;
  const mutedUntil = timeoutUntil(selfMember);
  const channel =
    groupState !== undefined && channelId !== null
      ? groupState.channels.find((c) => c.channel_id === channelId)
      : undefined;
  const readOnly =
    groupState !== undefined && channel !== undefined
      ? isChannelReadOnly(groupState, channel, self?.pubkey ?? null)
      : false;
  let notice: string | null = null;
  if (mutedUntil !== null) {
    notice = interpolate(t.groups.timedOutNotice, {
      time: formatTimestamp(mutedUntil, lang),
    });
  } else if (readOnly) {
    notice = t.groups.announcementReadOnly;
  }

  if (notice !== null) {
    return (
      <div className="px-4 pb-6">
        <div
          role="status"
          className="flex items-center gap-2.5 rounded-lg bg-input px-4 py-3 text-sm text-muted"
        >
          <svg
            width="18"
            height="18"
            viewBox="0 0 24 24"
            fill="currentColor"
            aria-hidden
            className="shrink-0"
          >
            <path d="M12 2a5 5 0 0 0-5 5v3H6a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7a2 2 0 0 0-2-2h-1V7a5 5 0 0 0-5-5Zm3 8H9V7a3 3 0 0 1 6 0v3Z" />
          </svg>
          <span>{notice}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="px-4 pb-6">
      {pieces.length > 0 && (
        <div className="mb-1 flex flex-wrap gap-2 rounded-t-lg bg-sidebar px-3 py-2">
          {pieces.map((piece) => (
            <div
              key={piece.id}
              className="relative flex items-center gap-2 rounded-lg bg-rail px-2 py-1.5"
            >
              {piece.url !== null ? (
                <img
                  src={piece.url}
                  alt={piece.file.name}
                  width={40}
                  height={40}
                  className="h-10 w-10 rounded object-cover"
                />
              ) : (
                <svg
                  width="22"
                  height="22"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                  aria-hidden
                  className="shrink-0 text-faint"
                >
                  <path d="M6 2a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8.8a2 2 0 0 0-.6-1.4l-4.8-4.8A2 2 0 0 0 13.2 2H6Zm7 1.5L18.5 9H14a1 1 0 0 1-1-1V3.5Z" />
                </svg>
              )}
              <div className="min-w-0 max-w-40">
                <div className="truncate text-xs font-medium text-header">
                  {piece.file.name}
                </div>
                <div className="text-[10px] text-faint">
                  {tailleLisible(piece.file.size, lang)}
                </div>
              </div>
              <button
                type="button"
                aria-label={interpolate(t.fichiers.retirerPiece, {
                  name: piece.file.name,
                })}
                title={interpolate(t.fichiers.retirerPiece, { name: piece.file.name })}
                disabled={sending}
                onClick={() => retirer(piece.id)}
                className="rounded-full p-0.5 text-faint hover:text-red disabled:opacity-40"
              >
                <svg
                  width="14"
                  height="14"
                  viewBox="0 0 24 24"
                  fill="currentColor"
                  aria-hidden
                >
                  <path d="M6.3 5 12 10.6 17.7 5 19 6.3 13.4 12l5.6 5.7-1.3 1.3-5.7-5.6L6.3 19 5 17.7l5.6-5.7L5 6.3 6.3 5Z" />
                </svg>
              </button>
            </div>
          ))}
          {publierEnCours && (
            <span className="self-center text-xs italic text-muted" role="status">
              {t.fichiers.publication}
            </span>
          )}
        </div>
      )}
      {erreur !== null && (
        <p className="mb-1 px-1 text-sm text-red" role="alert">
          {erreur}
        </p>
      )}
      <div
        className={`relative flex items-end rounded-lg bg-input ${
          survol ? 'ring-2 ring-blurple' : ''
        }`}
        onDragOver={(e) => {
          if (!e.dataTransfer.types.includes('Files')) return;
          e.preventDefault();
          setSurvol(true);
        }}
        onDragLeave={() => setSurvol(false)}
        onDrop={(e) => {
          e.preventDefault();
          setSurvol(false);
          ajouter(Array.from(e.dataTransfer.files));
        }}
      >
        {mentionOpen && (
          <MentionAutocomplete
            candidates={suggestions}
            activeIndex={activeIndex}
            onSelect={chooseMention}
            onHover={setMentionIndex}
          />
        )}
        <input
          ref={fileRef}
          type="file"
          multiple
          aria-label={t.fichiers.joindre}
          className="hidden"
          onChange={(e) => {
            const fichiers = Array.from(e.target.files ?? []);
            // Autorise de re-choisir le même fichier plus tard.
            e.target.value = '';
            ajouter(fichiers);
          }}
        />
        <button
          type="button"
          aria-label={t.fichiers.joindre}
          title={t.fichiers.joindre}
          disabled={sending}
          onClick={() => fileRef.current?.click()}
          className="m-1.5 rounded-md p-2 text-muted transition-colors enabled:hover:text-norm disabled:opacity-40"
        >
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
            <path d="M12 2a5 5 0 0 1 5 5v9a3.5 3.5 0 1 1-7 0V8a2 2 0 1 1 4 0v8a1 1 0 1 1-2 0V8a.5.5 0 0 0-.5.5V16a2 2 0 1 0 3.5 1.3V7a3 3 0 0 0-6 0v9.5a1 1 0 1 1-2 0V7a5 5 0 0 1 5-5Z" />
          </svg>
        </button>
        <textarea
          ref={textareaRef}
          aria-label={placeholder}
          placeholder={placeholder}
          value={text}
          rows={1}
          onChange={(e) => {
            const value = e.target.value;
            setText(value);
            notifyTyping(value);
            // Real edits recompute the mention and reset the highlight.
            setMention(findActiveMention(value, e.target.selectionStart ?? value.length));
            setMentionIndex(0);
          }}
          onClick={(e) => syncCaret(e.currentTarget)}
          onKeyUp={(e) => {
            if (['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) {
              syncCaret(e.currentTarget);
            }
          }}
          onPaste={(e) => {
            const fichiers = Array.from(e.clipboardData.files);
            if (fichiers.length === 0) return;
            e.preventDefault();
            ajouter(fichiers);
          }}
          onKeyDown={(e) => {
            // The autocomplete captures navigation keys before send/newline.
            if (mentionOpen) {
              if (e.key === 'ArrowDown') {
                e.preventDefault();
                setMentionIndex((i) => (i + 1) % suggestions.length);
                return;
              }
              if (e.key === 'ArrowUp') {
                e.preventDefault();
                setMentionIndex((i) => (i - 1 + suggestions.length) % suggestions.length);
                return;
              }
              if (e.key === 'Enter' || e.key === 'Tab') {
                e.preventDefault();
                chooseMention(suggestions[activeIndex]);
                return;
              }
              if (e.key === 'Escape') {
                e.preventDefault();
                setMention(null);
                return;
              }
            }
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              void submit();
            }
          }}
          className="max-h-48 min-h-[44px] flex-1 resize-none bg-transparent py-2.5 text-norm placeholder-faint outline-none"
        />
        <div className="relative">
          <button
            type="button"
            aria-label={t.emoji.open}
            title={t.emoji.open}
            aria-expanded={emojiOpen}
            disabled={sending}
            onClick={() => setEmojiOpen((open) => !open)}
            className={`m-1.5 rounded-md p-2 transition-colors disabled:opacity-40 ${
              emojiOpen ? 'text-blurple' : 'text-muted enabled:hover:text-norm'
            }`}
          >
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="currentColor"
              aria-hidden
            >
              <path d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16Zm-3.5-8.5A1.5 1.5 0 1 0 8.5 8.5a1.5 1.5 0 0 0 0 3Zm7 0a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3Zm-3.5 6c2.3 0 4.3-1.4 5.1-3.5H6.9c.8 2.1 2.8 3.5 5.1 3.5Z" />
            </svg>
          </button>
          {emojiOpen && (
            <EmojiPicker
              groupId={groupId}
              onSelect={(pick) => {
                setEmojiOpen(false);
                insererEmoji(pick);
              }}
              onClose={() => setEmojiOpen(false)}
            />
          )}
        </div>
        <button
          type="button"
          aria-label={t.app.send}
          title={t.app.send}
          disabled={(text.trim() === '' && pieces.length === 0) || sending}
          onClick={() => void submit()}
          className="m-1.5 rounded-md p-2 text-muted transition-colors enabled:hover:text-blurple disabled:opacity-40"
        >
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
            <path d="M3.4 20.4 20.9 12 3.4 3.6a.7.7 0 0 0-1 .8L4.5 12 2.4 19.6a.7.7 0 0 0 1 .8ZM6.2 13l9.2-1-9.2-1-1.2-4.4L18 12 5 17.4 6.2 13Z" />
          </svg>
        </button>
      </div>
    </div>
  );
}
