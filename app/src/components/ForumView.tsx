/**
 * Vue d'un salon FORUM. Un salon forum n'accueille pas de messages directs :
 * le nœud refuse tout envoi dans sa racine (voir `can_send_message`). Ses
 * « posts » sont des FILS (threads) ancrés au salon forum — l'infra threads est
 * réutilisée telle quelle. Cette vue liste les posts (actifs / archivés),
 * permet d'en créer un, et ouvre un post dans le `ThreadPanel` existant.
 *
 * Création d'un post : `groups.thread.create` sous un parent forum crée le fil ;
 * `root_msg` n'est PAS validé côté cœur (aucune racine préexistante n'est
 * requise) — on passe donc 16 octets zéro. Le premier message est ensuite
 * publié DANS le fil (`thread_id`), jamais dans la racine du forum.
 */

import { useEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { GroupChannel, GroupThread } from '../lib/api';
import { useGroups } from '../stores/groups';
import { useT, useUi } from '../stores/ui';
import type { DisplayMessage } from './MessageList';
import { ThreadPanel } from './ThreadPanel';

/**
 * `root_msg` d'un post de forum : 16 octets zéro (32 hex). Aucun message racine
 * n'existe dans la racine d'un forum, et le cœur ne valide pas ce champ.
 */
const FORUM_POST_ROOT = '0'.repeat(32);

/** Glyphe « forum » (barres empilées), aligné sur `ChannelIcon`. */
function ForumGlyph() {
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
      <path d="M17 6.1H3" />
      <path d="M21 12.1H3" />
      <path d="M15.1 18H3" />
    </svg>
  );
}

export interface ForumViewProps {
  groupId: string;
  /** Le salon forum courant (`kind === 'forum'`). */
  channel: GroupChannel;
  /** Posts du forum = `channelThreads(state, channel.channel_id)`. */
  posts: readonly GroupThread[];
  /** MANAGE_CHANNELS sur le forum : archivage des posts (via `ThreadPanel`). */
  canManage: boolean;
  /** MANAGE_MESSAGES : suppression des messages d'autrui dans un post. */
  canModerate: boolean;
  /** VIEW+SEND effectif sur le forum : autorise « Nouveau post ». */
  canPost: boolean;
  colorOf: (author: string) => string | null;
  emojiMap: ReadonlyMap<string, string>;
  knownMentions: ReadonlySet<string>;
  automodWords: readonly string[];
  nameOf: (author: string) => string;
}

export function ForumView({
  groupId,
  channel,
  posts,
  canManage,
  canModerate,
  canPost,
  colorOf,
  emojiMap,
  knownMentions,
  automodWords,
  nameOf,
}: ForumViewProps) {
  const t = useT();
  const createThread = useGroups((s) => s.createThread);
  const send = useGroups((s) => s.send);
  const toast = useUi((s) => s.toast);
  /** Post ouvert dans le panneau latéral (`null` : aucun). */
  const [openPostId, setOpenPostId] = useState<string | null>(null);
  /** Formulaire « Nouveau post » déplié. */
  const [showForm, setShowForm] = useState(false);
  const [title, setTitle] = useState('');
  const [firstMessage, setFirstMessage] = useState('');
  const [busy, setBusy] = useState(false);
  const newPostRef = useRef<HTMLButtonElement>(null);
  const titleRef = useRef<HTMLInputElement>(null);

  // Formulaire déplié : focus sur le titre pour enchaîner au clavier.
  useEffect(() => {
    if (showForm) titleRef.current?.focus();
  }, [showForm]);

  /** Referme le formulaire et rend le focus au bouton « Nouveau post ». */
  const fermerFormulaire = (): void => {
    setShowForm(false);
    newPostRef.current?.focus();
  };

  const forumId = channel.channel_id;
  const openPost =
    openPostId === null ? null : (posts.find((p) => p.thread_id === openPostId) ?? null);
  const active = posts.filter((p) => !p.archived);
  const archived = posts.filter((p) => p.archived);
  const canSubmit = title.trim() !== '' && firstMessage.trim() !== '' && !busy;

  const submit = async (): Promise<void> => {
    if (!canSubmit) return;
    setBusy(true);
    try {
      // Crée le fil (post) sous le forum, puis publie le 1er message DANS le
      // fil : la racine du forum refuse tout envoi direct.
      const threadId = await createThread(
        groupId,
        forumId,
        FORUM_POST_ROOT,
        title.trim(),
      );
      await send(groupId, threadId, firstMessage.trim());
      setTitle('');
      setFirstMessage('');
      setShowForm(false);
      setOpenPostId(threadId);
    } catch {
      toast('error', t.errors.actionFailed);
    } finally {
      setBusy(false);
    }
  };

  const postCard = (post: GroupThread) => (
    <button
      key={post.thread_id}
      type="button"
      aria-label={interpolate(t.forum.openPost, { name: post.name })}
      onClick={() => setOpenPostId(post.thread_id)}
      className="group flex w-full items-start gap-3 rounded-lg border border-[color:var(--glass-border)] bg-sidebar px-4 py-3 text-left transition-colors duration-fast hover:border-blurple/40 hover:bg-chat-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat active:scale-[0.99]"
    >
      <span
        aria-hidden
        className="mt-0.5 shrink-0 text-faint transition-colors duration-fast group-hover:text-blurple"
      >
        <ForumGlyph />
      </span>
      <span className="min-w-0 flex-1 truncate font-medium text-header">{post.name}</span>
      {post.archived && (
        <span className="shrink-0 rounded-full bg-chat-hover px-2 py-0.5 text-[11px] font-medium text-faint">
          {t.threads.archived}
        </span>
      )}
    </button>
  );

  return (
    <div className="flex h-full">
      <div className="relative flex min-w-0 flex-1 flex-col">
        <header className="flex h-12 shrink-0 items-center gap-2 border-b border-[color:var(--glass-border)] bg-chat/90 px-4 shadow-1">
          <span
            aria-hidden
            className="flex h-5 w-5 shrink-0 items-center justify-center text-faint"
          >
            <ForumGlyph />
          </span>
          <span
            className="min-w-0 truncate font-semibold text-header"
            title={channel.name}
          >
            {channel.name}
          </span>
          {channel.topic !== '' && (
            <>
              <span aria-hidden className="h-5 w-px shrink-0 bg-input" />
              <span className="min-w-0 truncate text-sm text-muted" title={channel.topic}>
                {channel.topic}
              </span>
            </>
          )}
          {canPost && (
            <button
              ref={newPostRef}
              type="button"
              aria-expanded={showForm}
              onClick={() => setShowForm((open) => !open)}
              className="ml-auto shrink-0 rounded-lg bg-blurple px-3 py-1.5 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat active:scale-95"
            >
              {t.forum.newPost}
            </button>
          )}
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-4 py-4">
          {showForm && canPost && (
            <form
              onSubmit={(e) => {
                e.preventDefault();
                void submit();
              }}
              onKeyDown={(e) => {
                if (e.key === 'Escape') {
                  e.stopPropagation();
                  fermerFormulaire();
                }
              }}
              className="mb-5 rounded-lg border border-[color:var(--glass-border)] bg-sidebar p-4 shadow-1"
            >
              <h2 className="mb-3 text-sm font-semibold text-header">
                {t.forum.newPostTitle}
              </h2>
              <input
                ref={titleRef}
                aria-label={t.forum.postTitle}
                placeholder={t.forum.postTitle}
                value={title}
                onChange={(e) => setTitle(e.target.value)}
                className="w-full rounded-md border border-transparent bg-input px-3 py-2.5 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
              />
              <textarea
                aria-label={t.forum.firstMessage}
                placeholder={t.forum.firstMessage}
                value={firstMessage}
                onChange={(e) => setFirstMessage(e.target.value)}
                rows={4}
                className="mt-3 w-full resize-y rounded-md border border-transparent bg-input px-3 py-2.5 text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
              />
              <div className="mt-3 flex justify-end gap-3">
                <button
                  type="button"
                  onClick={fermerFormulaire}
                  className="rounded-sm px-4 py-2 text-sm font-medium text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat"
                >
                  {t.app.cancel}
                </button>
                <button
                  type="submit"
                  disabled={!canSubmit}
                  className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-chat disabled:opacity-50 active:scale-[0.98]"
                >
                  {t.forum.publish}
                </button>
              </div>
            </form>
          )}
          {posts.length === 0 && !showForm && (
            <p className="py-16 text-center text-sm text-muted">{t.forum.emptyForum}</p>
          )}
          {active.length > 0 && (
            <section aria-labelledby="forum-active-heading">
              <h2
                id="forum-active-heading"
                className="px-1 pb-2 text-[11px] font-semibold uppercase tracking-wide text-faint"
              >
                {t.forum.activePosts}
              </h2>
              <div className="flex flex-col gap-2">{active.map(postCard)}</div>
            </section>
          )}
          {archived.length > 0 && (
            <section aria-labelledby="forum-archived-heading" className="mt-6">
              <h2
                id="forum-archived-heading"
                className="px-1 pb-2 text-[11px] font-semibold uppercase tracking-wide text-faint"
              >
                {t.forum.archivedPosts}
              </h2>
              <div className="flex flex-col gap-2">{archived.map(postCard)}</div>
            </section>
          )}
        </div>
      </div>
      {openPost !== null && (
        <ThreadPanel
          groupId={groupId}
          thread={openPost}
          // Un post de forum n'a pas de racine dans le salon (root_msg = zéro).
          rootMessage={undefined as DisplayMessage | undefined}
          canManage={canManage}
          canModerate={canModerate}
          colorOf={colorOf}
          emojiMap={emojiMap}
          knownMentions={knownMentions}
          automodWords={automodWords}
          nameOf={nameOf}
          onClose={() => setOpenPostId(null)}
        />
      )}
    </div>
  );
}
