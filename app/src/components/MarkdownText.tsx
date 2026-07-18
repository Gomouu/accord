/**
 * Rendu sûr du markdown léger : consomme l'arbre pur de `lib/markdown` et
 * produit des nœuds React (aucun `dangerouslySetInnerHTML` — React échappe le
 * texte). Compose émojis custom `:name:`, mentions `@pseudo` (surlignées, en
 * « pill » pour un membre connu) et mise en forme dans un même passage.
 *
 * Discord-level additions: headings, lists, blockquotes, underline, masked
 * links (real URL in the title attribute + distinct dotted underline) and
 * fenced code blocks highlighted by the zero-dependency `lib/highlight`
 * tokenizer. Token colors use the themed CSS variables (light and dark).
 */

import { Fragment, memo, useMemo, useState, type ReactNode } from 'react';
import { analyserMarkdown, type MdNode } from '../lib/markdown';
import { highlightCode, type TokenKind } from '../lib/highlight';
import { roleColorCss } from '../stores/groups';
import { useT, useUi, type EmojiSize } from '../stores/ui';
import { CustomEmoji } from './CustomEmoji';

/** Diamètre (px) d'un émoji personnalisé selon le réglage « Taille des émojis ». */
const CUSTOM_EMOJI_PX: Record<EmojiSize, number | undefined> = {
  normal: 28,
  large: 48,
};

/**
 * Séquences d'émojis unicode (pictogrammes, ZWJ, sélecteurs de variante,
 * teintes de peau) : agrandies dans le corps des messages via `.emoji-uni`
 * (D-054 — à la taille du texte, un émoji est illisible).
 */
const EMOJI_UNICODE =
  /((?![©®™])\p{Extended_Pictographic}(?:️|[\u{1F3FB}-\u{1F3FF}])?(?:‍\p{Extended_Pictographic}(?:️|[\u{1F3FB}-\u{1F3FF}])?)*)/gu;

/** Enveloppe chaque séquence d'émoji unicode d'un `<span>` agrandi. */
function texteAvecEmojis(value: string): ReactNode {
  if (!EMOJI_UNICODE.test(value)) return value;
  EMOJI_UNICODE.lastIndex = 0;
  const parts = value.split(EMOJI_UNICODE);
  return parts.map((part, i) =>
    i % 2 === 1 ? (
      <span key={i} className="emoji-uni">
        {part}
      </span>
    ) : (
      part
    ),
  );
}

export interface MarkdownTextProps {
  text: string;
  /** Émojis du serveur (nom → racine Merkle) pour rendre `:name:` en image. */
  emojis?: ReadonlyMap<string, string> | undefined;
  /** Noms connus (en minuscules) rendus en « pill » de mention. */
  knownMentions?: ReadonlySet<string> | undefined;
  /** Rôles connus (nom minuscule → couleur `0xRRGGBB`) rendus en pastille colorée. */
  roleColors?: ReadonlyMap<string, number> | undefined;
  /** Pair source probable pour le téléchargement des images d'émoji. */
  hint?: string | undefined;
}

/** Contexte de rendu passé récursivement aux nœuds. */
interface Ctx {
  emojis?: ReadonlyMap<string, string> | undefined;
  knownMentions?: ReadonlySet<string> | undefined;
  roleColors?: ReadonlyMap<string, number> | undefined;
  hint?: string | undefined;
  /** Taille des émojis personnalisés `:nom:` (Paramètres → Texte & médias). */
  emojiSize: EmojiSize;
}

/** Style « pill » d'un rôle : texte à sa couleur, fond translucide assorti. */
function roleMentionStyle(color: number): { color: string; backgroundColor: string } {
  const r = (color >> 16) & 0xff;
  const g = (color >> 8) & 0xff;
  const b = color & 0xff;
  return { color: roleColorCss(color), backgroundColor: `rgba(${r}, ${g}, ${b}, 0.16)` };
}

/** N'accepte que les schémas http/https (les autres sont rendus en texte). */
function lienSur(url: string): string | undefined {
  try {
    const p = new URL(url);
    if (p.protocol === 'http:' || p.protocol === 'https:') return url;
  } catch {
    // URL non analysable : traitée comme du texte par l'appelant.
  }
  return undefined;
}

/** Spoiler : contenu masqué révélé au clic ou au clavier (Entrée/Espace). */
function Spoiler({ children }: { children: ReactNode }) {
  const t = useT();
  const [revele, setRevele] = useState(false);
  if (revele) {
    return <span className="rounded-sm bg-input/70 px-0.5">{children}</span>;
  }
  return (
    <span
      role="button"
      tabIndex={0}
      aria-label={t.emoji.spoilerReveal}
      onClick={() => setRevele(true)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          setRevele(true);
        }
      }}
      className="cursor-pointer select-none rounded-sm bg-faint/60 px-0.5 text-transparent transition-colors duration-fast hover:bg-faint/70"
    >
      {children}
    </span>
  );
}

/**
 * Token colors for highlighted code, mapped to the themed CSS variables
 * (`--color-*` in styles/global.css) so both light and dark themes work.
 */
const TOKEN_CLASS: Record<Exclude<TokenKind, 'plain'>, string> = {
  keyword: 'text-blurple',
  string: 'text-green',
  comment: 'italic text-faint',
  number: 'text-yellow',
};

/**
 * Fenced code block. With a known language tag the content is tokenized by
 * `lib/highlight` into pure data tokens rendered as `<span>`s (never HTML
 * strings); unknown or missing languages render as plain text.
 */
function CodeBlock({ value, lang }: { value: string; lang?: string | undefined }) {
  const tokens = lang !== undefined ? highlightCode(value, lang) : null;
  return (
    <pre className="my-1 overflow-x-auto rounded-lg border border-rail/70 bg-input p-2.5 font-mono text-[0.85em] text-norm shadow-1">
      <code>
        {tokens === null
          ? value
          : tokens.map((tok, i) =>
              tok.kind === 'plain' ? (
                <Fragment key={i}>{tok.value}</Fragment>
              ) : (
                <span key={i} className={TOKEN_CLASS[tok.kind]}>
                  {tok.value}
                </span>
              ),
            )}
      </code>
    </pre>
  );
}

/** Heading sizes tuned to Discord's chat headings (h1 > h2 > h3). */
const HEADING_CLASS: Record<1 | 2 | 3, string> = {
  1: 'my-1 block text-2xl font-semibold leading-tight text-header',
  2: 'my-1 block text-xl font-semibold leading-tight text-header',
  3: 'my-1 block text-base font-semibold leading-tight text-header',
};

function renderNodes(nodes: readonly MdNode[], ctx: Ctx): ReactNode {
  return nodes.map((node, i) => <Fragment key={i}>{renderNode(node, ctx)}</Fragment>);
}

function renderNode(node: MdNode, ctx: Ctx): ReactNode {
  switch (node.type) {
    case 'text':
      return texteAvecEmojis(node.value);
    case 'break':
      return <br />;
    case 'bold':
      return <strong className="font-semibold">{renderNodes(node.children, ctx)}</strong>;
    case 'italic':
      return <em>{renderNodes(node.children, ctx)}</em>;
    case 'underline':
      return <u>{renderNodes(node.children, ctx)}</u>;
    case 'strike':
      return <s>{renderNodes(node.children, ctx)}</s>;
    case 'spoiler':
      return <Spoiler>{renderNodes(node.children, ctx)}</Spoiler>;
    case 'code':
      return (
        <code className="rounded-sm bg-rail/60 px-1 py-0.5 font-mono text-[0.85em] text-norm">
          {node.value}
        </code>
      );
    case 'codeblock':
      return <CodeBlock value={node.value} lang={node.lang} />;
    case 'heading': {
      const Tag = `h${node.level}` as 'h1' | 'h2' | 'h3';
      return (
        <Tag className={HEADING_CLASS[node.level]}>{renderNodes(node.children, ctx)}</Tag>
      );
    }
    case 'list': {
      const items = node.items.map((item, i) => (
        <li key={i}>{renderNodes(item, ctx)}</li>
      ));
      const cls = `my-0.5 list-outside pl-6 ${node.ordered ? 'list-decimal' : 'list-disc'}`;
      if (node.ordered) {
        return (
          <ol start={node.start} className={cls}>
            {items}
          </ol>
        );
      }
      return <ul className={cls}>{items}</ul>;
    }
    case 'blockquote':
      return (
        <blockquote className="relative my-0.5 pl-3 text-muted before:absolute before:inset-y-0 before:left-0 before:w-[3px] before:rounded-full before:bg-faint/50 before:content-['']">
          {renderNodes(node.children, ctx)}
        </blockquote>
      );
    case 'table':
      return (
        <div className="my-1 max-w-full overflow-x-auto">
          <table className="border-collapse text-sm">
            <thead>
              <tr>
                {node.header.map((cell, c) => (
                  <th
                    key={c}
                    className="border border-input bg-rail/40 px-2.5 py-1 font-semibold text-norm"
                    style={{ textAlign: node.align[c] ?? undefined }}
                  >
                    {renderNodes(cell, ctx)}
                  </th>
                ))}
              </tr>
            </thead>
            <tbody>
              {node.rows.map((row, r) => (
                <tr key={r}>
                  {row.map((cell, c) => (
                    <td
                      key={c}
                      className="border border-input px-2.5 py-1 text-norm"
                      style={{ textAlign: node.align[c] ?? undefined }}
                    >
                      {renderNodes(cell, ctx)}
                    </td>
                  ))}
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      );
    case 'link': {
      const href = lienSur(node.href);
      if (href === undefined) return node.value;
      return (
        <a
          href={href}
          target="_blank"
          rel="noopener noreferrer"
          className="text-blurple hover:underline"
        >
          {node.value}
        </a>
      );
    }
    case 'masklink': {
      const href = lienSur(node.href);
      // Defense in depth: the parser already restricts schemes to http(s).
      if (href === undefined) return renderNodes(node.children, ctx);
      return (
        <a
          href={href}
          title={href}
          target="_blank"
          rel="noopener noreferrer"
          className="text-blurple underline decoration-dotted underline-offset-2 hover:decoration-solid"
        >
          {renderNodes(node.children, ctx)}
        </a>
      );
    }
    case 'mention': {
      const lower = node.name.toLowerCase();
      // Broadcast mentions always read as a pill (distinct amber accent).
      if (lower === 'everyone' || lower === 'here') {
        return (
          <span className="rounded-xs bg-yellow/20 px-0.5 font-medium text-yellow">
            @{node.name}
          </span>
        );
      }
      // Role mention: coloured pill using the role's own colour when set.
      const roleColor = ctx.roleColors?.get(lower);
      if (roleColor !== undefined) {
        if (roleColor === 0) {
          return (
            <span className="rounded-xs bg-blurple/20 px-0.5 font-medium text-blurple">
              @{node.name}
            </span>
          );
        }
        return (
          <span
            className="rounded-xs px-0.5 font-medium"
            style={roleMentionStyle(roleColor)}
          >
            @{node.name}
          </span>
        );
      }
      const connu = ctx.knownMentions?.has(lower) ?? false;
      return (
        <span
          className={
            connu
              ? 'rounded-xs bg-blurple/20 px-0.5 font-medium text-blurple'
              : 'font-medium text-blurple'
          }
        >
          @{node.name}
        </span>
      );
    }
    case 'emoji': {
      const merkle = ctx.emojis?.get(node.name);
      if (merkle === undefined) return `:${node.name}:`;
      return (
        <CustomEmoji
          name={node.name}
          merkleRoot={merkle}
          hint={ctx.hint}
          size={CUSTOM_EMOJI_PX[ctx.emojiSize]}
        />
      );
    }
  }
}

/**
 * Rend un texte de message en nœuds React (markdown + émojis + mentions).
 * Mémoïsé : `analyserMarkdown` (l'analyse coûteuse) ne re-tourne que si le
 * texte change, et `memo` évite tout ré-rendu quand la vue parente se
 * re-rend sans que les props de ce message bougent (arrivée d'un autre
 * message, survol, édition d'une autre rangée…).
 */
function MarkdownTextInner({
  text,
  emojis,
  knownMentions,
  roleColors,
  hint,
}: MarkdownTextProps) {
  const emojiSize = useUi((s) => s.emojiSize);
  const nodes = useMemo(() => analyserMarkdown(text), [text]);
  return (
    <>{renderNodes(nodes, { emojis, knownMentions, roleColors, hint, emojiSize })}</>
  );
}

export const MarkdownText = memo(MarkdownTextInner);
