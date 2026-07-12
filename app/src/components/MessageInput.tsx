/**
 * Zone de saisie : Entrée envoie, Maj+Entrée insère un saut de ligne.
 * Pièces jointes : bouton trombone, glisser-déposer sur la zone de saisie,
 * collage d'image — aperçus retirables, bornes UI (10 pièces, 8 Mio chacune).
 * À l'envoi, chaque pièce est publiée via `files.share_bytes` (état
 * « publication… »), puis le message part avec ses références.
 */

import { useEffect, useMemo, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { FileAttachment, MsgBody } from '../lib/api';
import {
  estImage,
  fichierEnB64,
  fichierEnDataUrl,
  validerAjout,
} from '../lib/attachments';
import { containsFiltered } from '../lib/automod';
import { draftKey, readDraft, writeDraft } from '../lib/drafts';
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
import { formatTimestamp, formatDuration, tailleLisible } from '../lib/format';
import { applySlashCommand } from '../lib/slashCommands';
import { VoiceRecorder, voiceFileName } from '../lib/voiceRecorder';
import { useTypingEmitter, type TypingTarget } from '../hooks/useTypingEmitter';
import { useContextMenu, type ContextMenuItem } from '../stores/contextMenu';
import { useDms } from '../stores/dms';
import { useFriends, displayNameOf } from '../stores/friends';
import {
  channelKey,
  isChannelReadOnly,
  sortRoles,
  timeoutUntil,
  useGroups,
} from '../stores/groups';
import { useMessageEdit } from '../stores/messageEdit';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { CloseIcon } from './ContextMenu';
import { EmojiPicker } from './EmojiPicker';
import { MentionAutocomplete } from './MentionAutocomplete';

/** Forme minimale partagée par `DmMessage`/`GroupMessage` pour ArrowUp-édite. */
interface OwnEditCandidate {
  msg_id: string;
  author: string;
  deleted: boolean;
  body: MsgBody;
}

/** Dernier message de `selfPubkey` encore éditable (texte, non supprimé). */
function lastOwnEditableMessageId(
  messages: readonly OwnEditCandidate[] | undefined,
  selfPubkey: string | null,
): string | null {
  if (messages === undefined || selfPubkey === null) return null;
  for (let i = messages.length - 1; i >= 0; i--) {
    const m = messages.at(i);
    if (m === undefined) continue;
    if (!m.deleted && m.author === selfPubkey && m.body.type === 'text') return m.msg_id;
  }
  return null;
}

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
  /** Quand cette clé devient non-nulle (ex. msg_id d'une réponse), le champ prend le focus. */
  focusKey?: string | null;
  /**
   * Mots filtrés par l'AutoMod du serveur (vue groupe seulement) : un texte
   * saisi qui en contient déclenche un avertissement — sans bloquer l'envoi.
   */
  automodWords?: readonly string[] | undefined;
  /**
   * Échéance murale (ms) du mode lent : envoi désactivé et compte à rebours
   * affiché jusque-là. `null`/absent = pas de mode lent actif. Le tick d'une
   * seconde vit ici — l'expiration réactive l'envoi sans re-render du parent.
   */
  slowmodeUntilMs?: number | null;
}

let prochainId = 1;

export function MessageInput({
  placeholder,
  onSend,
  groupId = null,
  typingTarget,
  focusKey = null,
  automodWords,
  slowmodeUntilMs = null,
}: MessageInputProps) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const toast = useUi((s) => s.toast);
  const openModal = useUi((s) => s.openModal);
  const mentionInsert = useUi((s) => s.mentionInsert);
  const clearMentionInsert = useUi((s) => s.clearMentionInsert);
  /** Signale la frappe au pair/salon (throttlé, best effort). */
  const notifyTyping = useTypingEmitter(typingTarget);
  // Brouillon restauré au montage pour la cible courante : le texte non envoyé
  // survit au changement de vue comme au redémarrage (voir lib/drafts).
  const [text, setText] = useState(() => readDraft(draftKey(typingTarget)) ?? '');
  const [sending, setSending] = useState(false);
  const [pieces, setPieces] = useState<PieceEnAttente[]>([]);
  const [erreur, setErreur] = useState<string | null>(null);
  const [survol, setSurvol] = useState(false);
  const [emojiOpen, setEmojiOpen] = useState(false);
  const [recording, setRecording] = useState(false);
  const [elapsedMs, setElapsedMs] = useState(0);
  const fileRef = useRef<HTMLInputElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const recorderRef = useRef<VoiceRecorder | null>(null);
  /** Secondes restantes du mode lent (0 : envoi permis). */
  const [slowmodeRemaining, setSlowmodeRemaining] = useState(0);

  // Compte à rebours du mode lent : tick local d'une seconde, interval
  // nettoyé au changement d'échéance comme au démontage. Quand le compte
  // atteint zéro, l'interval s'arrête de lui-même — l'envoi se réactive sans
  // re-render global (la prop du parent n'a pas changé).
  useEffect(() => {
    if (slowmodeUntilMs === null) {
      setSlowmodeRemaining(0);
      return;
    }
    const restant = (): number =>
      Math.max(0, Math.ceil((slowmodeUntilMs - Date.now()) / 1000));
    setSlowmodeRemaining(restant());
    const id = window.setInterval(() => {
      const secondes = restant();
      setSlowmodeRemaining(secondes);
      if (secondes === 0) window.clearInterval(id);
    }, 1000);
    return () => window.clearInterval(id);
  }, [slowmodeUntilMs]);

  const slowmodeActive = slowmodeRemaining > 0;

  // Clé de brouillon de la conversation courante (valeur primitive : l'effet
  // ne se déclenche qu'au vrai changement de cible, pas à chaque re-render).
  const currentDraftKey = draftKey(typingTarget);
  // Clé à laquelle `text` appartient : mise à jour au chargement d'un brouillon
  // entrant, jamais pendant la frappe — sert d'ancre au garde-fou ci-dessous.
  const loadedDraftKeyRef = useRef(currentDraftKey);

  // Persistance du brouillon (texte seulement) : à chaque frappe le texte est
  // enregistré sous la clé courante ; vidé, la clé est effacée. L'envoi fait
  // `setText('')`, ce qui efface donc aussi le brouillon.
  useEffect(() => {
    writeDraft(loadedDraftKeyRef.current, text);
  }, [text]);

  // Changement de conversation sans démontage : le brouillon sortant est déjà
  // persisté (effet ci-dessus, à chaque frappe), il ne reste qu'à charger
  // l'entrant. Le garde-fou évite d'écraser un texte en cours quand la cible
  // n'a pas changé (nouvel objet `typingTarget`, même clé dérivée).
  useEffect(() => {
    if (loadedDraftKeyRef.current === currentDraftKey) return;
    loadedDraftKeyRef.current = currentDraftKey;
    setText(readDraft(currentDraftKey) ?? '');
  }, [currentDraftKey]);

  /* @mention autocomplete: candidates from the group state (members, roles
   * and the two broadcasts); no candidates in a direct message. */
  const groupState = useGroups((s) => (groupId !== null ? s.states[groupId] : undefined));
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);

  // ArrowUp sur composeur vide (édition du dernier message, voir plus bas) :
  // lit le fil de la conversation courante dans le magasin dms/groups
  // concerné — jamais les deux à la fois (un seul `typingTarget`).
  const dmMessages = useDms((s) =>
    typingTarget?.kind === 'dm' ? s.conversations[typingTarget.peer] : undefined,
  );
  const groupMessages = useGroups((s) =>
    typingTarget?.kind === 'group'
      ? s.messages[channelKey(typingTarget.groupId, typingTarget.channelId)]
      : undefined,
  );

  const candidates = useMemo<MentionCandidate[]>(() => {
    if (groupState === undefined) return [];
    const nameOf = (pubkey: string): string =>
      self !== null && pubkey === self.pubkey
        ? selfDisplayName(self)
        : displayNameOf(contacts, pubkey);
    return groupMentionCandidates(
      groupState.members,
      sortRoles(groupState.roles),
      nameOf,
    );
  }, [groupState, contacts, self]);

  const [mention, setMention] = useState<ActiveMention | null>(null);
  const [mentionIndex, setMentionIndex] = useState(0);
  const suggestions = mention !== null ? filterMentions(candidates, mention.query) : [];
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

  // Mention demandée depuis un menu contextuel (message, membre) : réutilise
  // `insertMention` en simulant un jeton vide en fin de texte, avec un espace
  // séparateur si le texte courant n'en finit pas déjà un.
  useEffect(() => {
    if (mentionInsert === null) return;
    const candidate: MentionCandidate = {
      id: `context:${mentionInsert.name}`,
      value: mentionInsert.name,
      label: mentionInsert.name,
      kind: 'member',
    };
    setText((prev) => {
      const base = prev === '' || /\s$/.test(prev) ? prev : `${prev} `;
      const active: ActiveMention = { start: base.length, query: '' };
      return insertMention(base, active, candidate).text;
    });
    clearMentionInsert();
    const el = textareaRef.current;
    requestAnimationFrame(() => el?.focus());
  }, [mentionInsert, clearMentionInsert]);

  // Focus demandé par le parent (ex. clic sur « Répondre ») : dès que la clé
  // pointe un message, le champ prend le focus sans second clic.
  useEffect(() => {
    if (focusKey === null) return;
    const el = textareaRef.current;
    requestAnimationFrame(() => el?.focus());
  }, [focusKey]);

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
    if ((trimmed === '' && pieces.length === 0) || sending || slowmodeActive) return;
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
      // Commandes slash (`/shrug`, `/me`…) : transforme le texte final juste
      // avant l'envoi, sans toucher à l'état affiché ni à la saisie en cours.
      await onSend(applySlashCommand(trimmed), attachments.length > 0 ? attachments : undefined);
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

  /**
   * Publie l'enregistrement fini (blob → base64 → `files.share_bytes`) et
   * envoie immédiatement un message dédié (texte vide, une pièce audio) —
   * même chemin de publication que `submit()`, indépendant de `pieces`.
   */
  const envoyerVocal = async (blob: Blob, mime: string): Promise<void> => {
    setRecording(false);
    setElapsedMs(0);
    setSending(true);
    try {
      const nom = voiceFileName(mime);
      const dataB64 = await fichierEnB64(blob);
      const { file } = await api.filesShareBytes(nom, mime, dataB64);
      await onSend('', [file]);
      setErreur(null);
    } catch {
      setErreur(t.errors.sendFailed);
    } finally {
      setSending(false);
    }
  };

  /** Démarre l'enregistrement d'un message vocal (bouton micro, click-toggle). */
  const demarrerEnregistrement = (): void => {
    if (sending || recording) return;
    setErreur(null);
    setElapsedMs(0);
    setRecording(true);
    const recorder = new VoiceRecorder({
      onTick: (ms) => setElapsedMs(ms),
      onStop: (result) => {
        recorderRef.current = null;
        if (result.reason !== 'manual') {
          toast('info', t.vocal.limiteAtteinte);
        }
        void envoyerVocal(result.blob, result.mime);
      },
      onError: (error) => {
        recorderRef.current = null;
        setRecording(false);
        setElapsedMs(0);
        toast(
          'error',
          error === 'permission_denied' ? t.vocal.permissionRefusee : t.vocal.nonSupporte,
        );
      },
    });
    recorderRef.current = recorder;
    void recorder.start();
  };

  /** Annule l'enregistrement en cours (bouton corbeille) : octets jetés, micro relâché. */
  const annulerEnregistrement = (): void => {
    recorderRef.current?.cancel();
    recorderRef.current = null;
    setRecording(false);
    setElapsedMs(0);
  };

  /** Arrête l'enregistrement en cours (bouton coche) : finalise puis envoie via `onStop`. */
  const arreterEtEnvoyer = (): void => {
    recorderRef.current?.stop();
  };

  // Micro jamais laissé ouvert : annule toute capture en cours au démontage
  // (changement de conversation, fermeture de l'app).
  useEffect(() => {
    return () => {
      recorderRef.current?.cancel();
    };
  }, []);

  const publierEnCours = sending && pieces.length > 0;

  // Avertissement émetteur AutoMod : le texte saisi contient un mot filtré du
  // serveur — purement informatif, l'envoi reste permis (le masquage se fait
  // au rendu chez les clients honnêtes).
  const automodWarn =
    automodWords !== undefined &&
    automodWords.length > 0 &&
    containsFiltered(text, automodWords);

  // Composeur en lecture seule : sourdine active de l'utilisateur local, ou
  // salon d'annonces sans MANAGE_CHANNELS effectif. Le fil reste consultable.
  const channelId = typingTarget?.kind === 'group' ? typingTarget.channelId : null;

  /**
   * Envoie un sticker choisi dans le sélecteur : message dédié, immédiat —
   * jamais inséré dans le composeur (contrat `groups.send` étendu : un appel
   * ne peut pas mélanger texte et sticker).
   */
  const envoyerSticker = (name: string): void => {
    if (groupId === null || channelId === null) return;
    useGroups
      .getState()
      .sendSticker(groupId, channelId, name)
      .catch(() => setErreur(t.errors.sendFailed));
  };

  /**
   * Bouton trombone : ouvre directement le sélecteur de fichiers en MP (pas
   * de sondage possible hors groupe). En salon de groupe, révèle un petit
   * menu « Fichier » / « Sondage » (sondages réservés aux salons de groupe,
   * D-048 — jamais proposés en MP) via le menu contextuel partagé
   * (`.glass-strong`, navigation clavier déjà gérée par `ContextMenu`).
   */
  const openAttachMenu = (e: React.MouseEvent<HTMLButtonElement>): void => {
    if (groupId === null || channelId === null) {
      fileRef.current?.click();
      return;
    }
    const r = e.currentTarget.getBoundingClientRect();
    const items: ContextMenuItem[] = [
      {
        label: t.groups.pollMenuFile,
        icon: (
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden
          >
            <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48" />
          </svg>
        ),
        onClick: () => fileRef.current?.click(),
      },
      {
        label: t.groups.pollMenuPoll,
        icon: (
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={2}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden
          >
            <line x1="18" x2="18" y1="20" y2="10" />
            <line x1="12" x2="12" y1="20" y2="4" />
            <line x1="6" x2="6" y1="20" y2="14" />
          </svg>
        ),
        onClick: () => openModal({ kind: 'createPoll', groupId, channelId }),
      },
    ];
    useContextMenu.getState().openMenu(r.left, r.top, items);
  };

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
          className="flex items-center gap-2.5 rounded-xl bg-input px-4 py-3 text-sm text-muted"
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
            className="shrink-0"
          >
            <rect width="18" height="11" x="3" y="11" rx="2" ry="2" />
            <path d="M7 11V7a5 5 0 0 1 10 0v4" />
          </svg>
          <span>{notice}</span>
        </div>
      </div>
    );
  }

  return (
    <div className="px-4 pb-6">
      {pieces.length > 0 && (
        <div className="flex flex-wrap gap-2 rounded-t-xl border border-b-0 border-rail/60 bg-sidebar px-3 py-2.5">
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
                  className="h-10 w-10 rounded-md object-cover"
                />
              ) : (
                <svg
                  width="22"
                  height="22"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth={2}
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  aria-hidden
                  className="shrink-0 text-faint"
                >
                  <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
                  <path d="M14 2v4a2 2 0 0 0 2 2h4" />
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
                className="rounded-full p-0.5 text-faint transition-colors duration-fast hover:text-red focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar disabled:opacity-40"
              >
                <CloseIcon size={14} />
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
      {automodWarn && (
        <p className="mb-1 px-1 text-sm text-yellow" role="status">
          {t.automod.senderWarning}
        </p>
      )}
      <div
        className={`relative flex items-end gap-0.5 rounded-xl border bg-input px-1.5 py-1 shadow-1 transition-colors duration-fast focus-within:border-blurple/50 ${
          pieces.length > 0 ? 'rounded-t-none' : ''
        } ${survol ? 'border-blurple/50' : 'border-rail/60'}`}
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
        {mentionOpen && !recording && (
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
          disabled={sending || recording}
          onClick={openAttachMenu}
          className="m-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-muted transition-all duration-fast enabled:hover:scale-105 enabled:hover:bg-chat-hover enabled:hover:text-norm enabled:active:scale-95 disabled:opacity-40"
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
            <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48" />
          </svg>
        </button>
        {recording ? (
          <div
            aria-hidden
            className="m-0.5 flex h-10 w-10 shrink-0 items-center justify-center"
          >
            <span className="h-2.5 w-2.5 rounded-full bg-red animate-pulse" />
          </div>
        ) : (
          <button
            type="button"
            aria-label={t.vocal.enregistrer}
            title={t.vocal.enregistrer}
            disabled={sending}
            onClick={demarrerEnregistrement}
            className="m-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-muted transition-all duration-fast enabled:hover:scale-105 enabled:hover:bg-chat-hover enabled:hover:text-norm enabled:active:scale-95 disabled:opacity-40"
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
              <path d="M12 2a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z" />
              <path d="M19 10v1a7 7 0 0 1-14 0v-1" />
              <line x1="12" x2="12" y1="18" y2="22" />
              <line x1="8" x2="16" y1="22" y2="22" />
            </svg>
          </button>
        )}
        {recording && (
          <span
            role="status"
            aria-label={interpolate(t.vocal.enCours, {
              time: formatDuration(elapsedMs / 1000),
            })}
            className="mx-1 shrink-0 select-none text-[13px] tabular-nums text-muted"
          >
            {formatDuration(elapsedMs / 1000)}
          </span>
        )}
        <textarea
          ref={textareaRef}
          aria-label={placeholder}
          placeholder={placeholder}
          value={text}
          disabled={recording}
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
            // Composeur vide + Haut : édite le dernier de ses propres
            // messages texte (comportement Discord). N'agit jamais si
            // l'autocomplétion de mentions est ouverte (gérée ci-dessus) ni
            // si le composeur contient déjà du texte — sinon la navigation
            // native du curseur resterait sans effet de toute façon (champ
            // vide), donc rien n'est perdu à intercepter la touche ici.
            if (e.key === 'ArrowUp' && !mentionOpen && text === '') {
              const ownMessages = typingTarget?.kind === 'dm' ? dmMessages : groupMessages;
              const msgId = lastOwnEditableMessageId(ownMessages, self?.pubkey ?? null);
              if (msgId !== null) {
                e.preventDefault();
                useMessageEdit.getState().requestEdit(msgId);
              }
              return;
            }
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              void submit();
            }
          }}
          className="max-h-48 min-h-[40px] flex-1 resize-none self-center bg-transparent px-1 py-2 text-[15px] leading-5 text-norm placeholder-faint outline-none"
        />
        {slowmodeActive && !recording && (
          <span
            role="status"
            title={interpolate(t.groups.slowmodeWait, {
              seconds: String(slowmodeRemaining),
            })}
            aria-label={interpolate(t.groups.slowmodeWait, {
              seconds: String(slowmodeRemaining),
            })}
            className="mx-1 flex shrink-0 select-none items-center gap-1 self-center text-[13px] tabular-nums text-muted"
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth={2}
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <circle cx="12" cy="12" r="10" />
              <polyline points="12 6 12 12 16 14" />
            </svg>
            <span aria-hidden>{slowmodeRemaining}s</span>
          </span>
        )}
        {recording ? (
          <>
            <button
              type="button"
              aria-label={t.vocal.annuler}
              title={t.vocal.annuler}
              onClick={annulerEnregistrement}
              className="m-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-muted transition-all duration-fast hover:scale-105 hover:bg-chat-hover hover:text-red active:scale-95"
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
            </button>
            <button
              type="button"
              aria-label={t.vocal.envoyer}
              title={t.vocal.envoyer}
              onClick={arreterEtEnvoyer}
              className="m-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-muted transition-all duration-fast hover:scale-105 hover:bg-chat-hover hover:text-blurple active:scale-95"
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
                <polyline points="20 6 9 17 4 12" />
              </svg>
            </button>
          </>
        ) : (
          <>
            <div className="relative">
              <button
                type="button"
                aria-label={t.emoji.open}
                title={t.emoji.open}
                aria-expanded={emojiOpen}
                disabled={sending}
                onClick={() => setEmojiOpen((open) => !open)}
                className={`m-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-full transition-all duration-fast disabled:opacity-40 ${
                  emojiOpen
                    ? 'bg-blurple/15 text-blurple'
                    : 'text-muted enabled:hover:scale-105 enabled:hover:bg-chat-hover enabled:hover:text-norm enabled:active:scale-95'
                }`}
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
              </button>
              {emojiOpen && (
                <EmojiPicker
                  groupId={groupId}
                  onSelect={(pick) => {
                    setEmojiOpen(false);
                    insererEmoji(pick);
                  }}
                  onClose={() => setEmojiOpen(false)}
                  {...(groupId !== null && channelId !== null
                    ? {
                        onPickSticker: (name: string) => {
                          setEmojiOpen(false);
                          envoyerSticker(name);
                        },
                      }
                    : {})}
                />
              )}
            </div>
            <button
              type="button"
              aria-label={t.app.send}
              title={t.app.send}
              disabled={
                (text.trim() === '' && pieces.length === 0) || sending || slowmodeActive
              }
              onClick={() => void submit()}
              className="m-0.5 flex h-10 w-10 shrink-0 items-center justify-center rounded-full text-muted transition-all duration-fast enabled:hover:scale-105 enabled:hover:bg-chat-hover enabled:hover:text-blurple enabled:active:scale-95 disabled:opacity-40"
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
                <path d="m22 2-7 20-4-9-9-4Z" />
                <path d="M22 2 11 13" />
              </svg>
            </button>
          </>
        )}
      </div>
    </div>
  );
}
