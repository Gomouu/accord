/**
 * Markdown léger PUR (aucun React, aucune manipulation du DOM) : transforme le
 * texte d'un message en un arbre de nœuds de données que `components/MarkdownText`
 * rend sans jamais injecter de HTML. Un seul passage compose émojis custom,
 * mise en forme et mentions.
 *
 * Constructions gérées :
 * - gras `**…**`, italique `*…*` ou `_…_`, souligné `__…__`, barré `~~…~~`,
 *   spoiler `||…||` ;
 * - code inline `` `…` `` et bloc de code ```` ```…``` ```` (contenu littéral,
 *   étiquette de langage optionnelle après la clôture ouvrante) ;
 * - liens automatiques `http(s)://…` et liens masqués `[label](https://…)`
 *   (schéma restreint à http/https, sûrs au rendu) ;
 * - mentions `@pseudo` et émojis custom `:name:` ;
 * - échappement par `\` et sauts de ligne ;
 * - constructions de début de ligne (comme Discord) : titres `#`/`##`/`###`,
 *   listes `-`/`*`/`1.` (un niveau d'imbrication), citations `> ` et `>>> `.
 */

/** Nœud de l'arbre markdown (données pures, sérialisables). */
export type MdNode =
  | { readonly type: 'text'; readonly value: string }
  | { readonly type: 'bold'; readonly children: MdNode[] }
  | { readonly type: 'italic'; readonly children: MdNode[] }
  | { readonly type: 'underline'; readonly children: MdNode[] }
  | { readonly type: 'strike'; readonly children: MdNode[] }
  | { readonly type: 'spoiler'; readonly children: MdNode[] }
  | { readonly type: 'code'; readonly value: string }
  | { readonly type: 'codeblock'; readonly value: string; readonly lang?: string }
  | { readonly type: 'link'; readonly href: string; readonly value: string }
  | { readonly type: 'masklink'; readonly href: string; readonly children: MdNode[] }
  | { readonly type: 'mention'; readonly name: string }
  | { readonly type: 'emoji'; readonly name: string }
  | {
      readonly type: 'heading';
      readonly level: 1 | 2 | 3;
      readonly children: MdNode[];
    }
  | {
      readonly type: 'list';
      readonly ordered: boolean;
      /** Numéro du premier élément (listes ordonnées ; `1` sinon). */
      readonly start: number;
      /** Un élément peut contenir un nœud `list` imbriqué (un seul niveau). */
      readonly items: MdNode[][];
    }
  | { readonly type: 'blockquote'; readonly children: MdNode[] }
  | { readonly type: 'checkbox'; readonly checked: boolean }
  | {
      readonly type: 'table';
      /** Alignement par colonne (`null` = défaut, aligné selon la langue). */
      readonly align: readonly TableAlign[];
      /** Cellules d'en-tête (contenu inline). */
      readonly header: MdNode[][];
      /** Lignes de corps, chacune alignée sur la largeur de l'en-tête. */
      readonly rows: MdNode[][][];
    }
  | { readonly type: 'break' };

/** Alignement d'une colonne de tableau GFM. */
export type TableAlign = 'left' | 'center' | 'right' | null;

/** Profondeur maximale d'imbrication (garde anti-récursion pathologique). */
const MAX_DEPTH = 8;

/** Vrai si `c` n'est pas un caractère de mot (lettre ou chiffre Unicode). */
function estBord(c: string | undefined): boolean {
  return c === undefined || !/[\p{L}\p{N}]/u.test(c);
}

/**
 * Position de la prochaine occurrence non échappée de `delim` à partir de
 * `start`, ou `-1`. Les délimiteurs précédés de `\` sont ignorés.
 */
function trouverFermeture(src: string, delim: string, start: number): number {
  let i = start;
  while (i <= src.length - delim.length) {
    if (src[i] === '\\') {
      i += 2;
      continue;
    }
    if (src.startsWith(delim, i)) return i;
    i += 1;
  }
  return -1;
}

/** Retire un unique saut de ligne de tête et de fin d'un bloc de code. */
function contenuBloc(inner: string): string {
  return inner.replace(/^\n/, '').replace(/\n$/, '');
}

/** Lit une URL http(s) à partir de `i`, ponctuation de fin exclue, ou `null`. */
function lireLien(src: string, i: number): string | null {
  let j = i;
  while (j < src.length && !/\s/.test(src[j] ?? '') && src[j] !== '<' && src[j] !== '>') {
    j += 1;
  }
  const url = src.slice(i, j).replace(/[.,;:!?)\]}'"]+$/, '');
  return /^https?:\/\/\S/.test(url) ? url : null;
}

/** Language tag on the opening fence line: ```js\n… (letters/digits/+#.-_). */
const FENCE_LANG_RE = /^([A-Za-z0-9_+#.-]{1,24})\n/;

/** Builds a code-block node, extracting an optional leading language tag. */
function lireBlocCode(inner: string): MdNode {
  const m = FENCE_LANG_RE.exec(inner);
  const lang = m?.[1];
  if (m !== null && lang !== undefined) {
    return { type: 'codeblock', value: contenuBloc(inner.slice(m[0].length)), lang };
  }
  return { type: 'codeblock', value: contenuBloc(inner) };
}

/**
 * Reads a masked link `[label](https://url)` starting at `i` (pointing at the
 * `[`). Only non-empty single-line labels and http(s) URLs are accepted;
 * anything else stays literal text (the caller keeps scanning).
 */
function lireLienMasque(
  src: string,
  i: number,
  depth: number,
): { readonly node: MdNode; readonly next: number } | null {
  const labelEnd = trouverFermeture(src, '](', i + 1);
  if (labelEnd <= i + 1) return null;
  const label = src.slice(i + 1, labelEnd);
  if (label.includes('\n')) return null;
  const urlEnd = src.indexOf(')', labelEnd + 2);
  if (urlEnd === -1) return null;
  const url = src.slice(labelEnd + 2, urlEnd);
  if (!/^https?:\/\/\S+$/.test(url)) return null;
  return {
    node: { type: 'masklink', href: url, children: analyserFragment(label, depth + 1) },
    next: urlEnd + 1,
  };
}

/** Analyse récursive d'un fragment en nœuds inline. */
function analyserFragment(src: string, depth: number): MdNode[] {
  const nodes: MdNode[] = [];
  let buf = '';
  const flush = (): void => {
    if (buf !== '') {
      nodes.push({ type: 'text', value: buf });
      buf = '';
    }
  };
  const enveloppe = (
    type: 'bold' | 'italic' | 'underline' | 'strike' | 'spoiler',
    inner: string,
  ): void => {
    flush();
    nodes.push({ type, children: analyserFragment(inner, depth + 1) });
  };

  let i = 0;
  const n = src.length;
  while (i < n) {
    const c = src[i] ?? '';

    // Échappement : le caractère suivant est littéral.
    if (c === '\\' && i + 1 < n) {
      buf += src[i + 1];
      i += 2;
      continue;
    }

    // Bloc de code ```…``` (contenu littéral, étiquette de langage en tête).
    if (src.startsWith('```', i)) {
      const end = src.indexOf('```', i + 3);
      if (end !== -1) {
        flush();
        nodes.push(lireBlocCode(src.slice(i + 3, end)));
        i = end + 3;
        continue;
      }
    }

    // Code inline `…` (contenu littéral).
    if (c === '`') {
      const end = src.indexOf('`', i + 1);
      if (end > i + 1) {
        flush();
        nodes.push({ type: 'code', value: src.slice(i + 1, end) });
        i = end + 1;
        continue;
      }
    }

    if (depth < MAX_DEPTH) {
      // Spoiler ||…||.
      if (src.startsWith('||', i)) {
        const end = trouverFermeture(src, '||', i + 2);
        if (end > i + 2) {
          enveloppe('spoiler', src.slice(i + 2, end));
          i = end + 2;
          continue;
        }
      }
      // Gras **…**.
      if (src.startsWith('**', i)) {
        const end = trouverFermeture(src, '**', i + 2);
        if (end > i + 2) {
          enveloppe('bold', src.slice(i + 2, end));
          i = end + 2;
          continue;
        }
      }
      // Underline __…__ (checked before single `_` italic).
      if (src.startsWith('__', i)) {
        const end = trouverFermeture(src, '__', i + 2);
        if (end > i + 2) {
          enveloppe('underline', src.slice(i + 2, end));
          i = end + 2;
          continue;
        }
      }
      // Barré ~~…~~.
      if (src.startsWith('~~', i)) {
        const end = trouverFermeture(src, '~~', i + 2);
        if (end > i + 2) {
          enveloppe('strike', src.slice(i + 2, end));
          i = end + 2;
          continue;
        }
      }
      // Italique *…*.
      if (c === '*') {
        const end = trouverFermeture(src, '*', i + 1);
        if (end > i + 1) {
          enveloppe('italic', src.slice(i + 1, end));
          i = end + 1;
          continue;
        }
      }
      // Italique _…_ (garde de limite de mot : `snake_case` reste littéral).
      if (c === '_' && estBord(src[i - 1])) {
        const end = trouverFermeture(src, '_', i + 1);
        if (end > i + 1 && estBord(src[end + 1])) {
          enveloppe('italic', src.slice(i + 1, end));
          i = end + 1;
          continue;
        }
      }
      // Masked link [label](https://…) — http(s) only.
      if (c === '[') {
        const masked = lireLienMasque(src, i, depth);
        if (masked !== null) {
          flush();
          nodes.push(masked.node);
          i = masked.next;
          continue;
        }
      }
    }

    // Lien automatique http(s)://….
    if (src.startsWith('http://', i) || src.startsWith('https://', i)) {
      const lien = lireLien(src, i);
      if (lien !== null) {
        flush();
        nodes.push({ type: 'link', href: lien, value: lien });
        i += lien.length;
        continue;
      }
    }

    // Mention @pseudo.
    if (c === '@') {
      const m = /^@([\p{L}\p{N}_-]{1,32})/u.exec(src.slice(i));
      if (m?.[1] !== undefined) {
        flush();
        nodes.push({ type: 'mention', name: m[1] });
        i += m[0].length;
        continue;
      }
    }

    // Émoji custom :name:.
    if (c === ':') {
      const m = /^:([a-z0-9_]{2,32}):/.exec(src.slice(i));
      if (m?.[1] !== undefined) {
        flush();
        nodes.push({ type: 'emoji', name: m[1] });
        i += m[0].length;
        continue;
      }
    }

    // Saut de ligne.
    if (c === '\n') {
      flush();
      nodes.push({ type: 'break' });
      i += 1;
      continue;
    }

    buf += c;
    i += 1;
  }

  flush();
  return nodes;
}

/*
 * ── Block-level layer ────────────────────────────────────────────────────
 * Discord-style line-start constructs. Runs once at the top level: it walks
 * the message line by line, emits heading/list/blockquote nodes, and hands
 * everything else to the inline parser above. Code fences opened inside a
 * paragraph are consumed verbatim so their content is never read as markup.
 */

/** `# `, `## `, `### ` followed by non-empty content. */
const HEADING_RE = /^(#{1,3}) +(.*)$/;

/** `- x`, `* x` or `1. x`; leading spaces nest one level. */
const LIST_ITEM_RE = /^( *)(?:[-*]|(\d{1,9})\.) (.*)$/;

interface RawItem {
  readonly nested: boolean;
  readonly ordered: boolean;
  readonly start: number;
  readonly text: string;
}

/** Parses one line as a list item, or `null` when it is not one. */
function lireItem(line: string): RawItem | null {
  const m = LIST_ITEM_RE.exec(line);
  if (m === null) return null;
  const digits = m[2];
  return {
    nested: (m[1] ?? '').length > 0,
    ordered: digits !== undefined,
    start: digits !== undefined ? Number.parseInt(digits, 10) : 1,
    text: m[3] ?? '',
  };
}

/**
 * Builds a list node from consecutive raw items. Indented items attach as a
 * nested list (single level) at the end of the previous top-level item; an
 * indented item with no parent degrades to a top-level item.
 */
/** Case à cocher GFM en tête d'item : `[ ] `, `[x] ` ou `[X] `. */
const TASK_RE = /^\[([ xX])\]\s+([\s\S]*)$/;

/**
 * Nœuds inline d'un élément de liste. Une case à cocher GFM en tête est
 * extraite en nœud `checkbox` (le reste du texte est analysé normalement).
 */
function itemNodes(text: string): MdNode[] {
  const m = TASK_RE.exec(text);
  if (m !== null && m[1] !== undefined && m[2] !== undefined) {
    return [
      { type: 'checkbox', checked: m[1].toLowerCase() === 'x' },
      ...analyserFragment(m[2], 0),
    ];
  }
  return analyserFragment(text, 0);
}

function construireListe(items: readonly RawItem[]): MdNode {
  const topItems: MdNode[][] = [];
  let pending: RawItem[] = [];
  const flushNested = (): void => {
    if (pending.length === 0) return;
    const sub: MdNode = {
      type: 'list',
      ordered: pending[0]?.ordered ?? false,
      start: pending[0]?.start ?? 1,
      items: pending.map((it) => itemNodes(it.text)),
    };
    const parent = topItems[topItems.length - 1];
    if (parent !== undefined) parent.push(sub);
    pending = [];
  };
  for (const item of items) {
    if (item.nested && topItems.length > 0) {
      pending.push(item);
    } else {
      flushNested();
      topItems.push(itemNodes(item.text));
    }
  }
  flushNested();
  return {
    type: 'list',
    ordered: items[0]?.ordered ?? false,
    start: items[0]?.start ?? 1,
    items: topItems,
  };
}

/** End index (exclusive) of the line starting at `i`. */
function finDeLigne(src: string, i: number): number {
  const nl = src.indexOf('\n', i);
  return nl === -1 ? src.length : nl;
}

/**
 * End index of a paragraph "line". Mirrors the inline parser, where ``` and
 * `` ` `` scan across newlines: a code span or fence opened on this line and
 * closed on a later one extends the paragraph so fenced content is never
 * misread as a heading, list or quote marker.
 */
function finDeParagraphe(src: string, start: number, firstEnd: number): number {
  let end = firstEnd;
  let k = start;
  while (k < end) {
    const c = src[k];
    if (c === '\\') {
      k += 2;
      continue;
    }
    if (src.startsWith('```', k)) {
      const close = src.indexOf('```', k + 3);
      if (close === -1) break; // unclosed fence: stays literal on this line
      k = close + 3;
      if (k > end) end = finDeLigne(src, k);
      continue;
    }
    if (c === '`') {
      const close = src.indexOf('`', k + 1);
      if (close > k + 1) {
        k = close + 1;
        if (k > end) end = finDeLigne(src, k);
        continue;
      }
    }
    k += 1;
  }
  return end;
}

/** Transforme un texte de message en arbre markdown (fonction pure). */
/**
 * Découpe une ligne de tableau en cellules sur les `|` non échappés. Les `|`
 * de bord (optionnels en GFM) sont retirés ; `\|` produit un `|` littéral.
 */
function decouperCellules(line: string): string[] {
  let s = line.trim();
  if (s.startsWith('|')) s = s.slice(1);
  if (s.endsWith('|') && !s.endsWith('\\|')) s = s.slice(0, -1);
  const cells: string[] = [];
  let cur = '';
  for (let k = 0; k < s.length; k++) {
    const ch = s[k];
    if (ch === '\\' && s[k + 1] === '|') {
      cur += '|';
      k++;
      continue;
    }
    if (ch === '|') {
      cells.push(cur.trim());
      cur = '';
      continue;
    }
    cur += ch;
  }
  cells.push(cur.trim());
  return cells;
}

/**
 * Interprète une ligne de séparateur de tableau (`| --- | :--: |`) en
 * alignements de colonnes, ou `null` si ce n'est pas un séparateur valide.
 */
function lireAlignements(line: string): TableAlign[] | null {
  const cells = decouperCellules(line);
  if (cells.length === 0 || cells.some((c) => c === '')) return null;
  const aligns: TableAlign[] = [];
  for (const c of cells) {
    if (!/^:?-+:?$/.test(c)) return null;
    const left = c.startsWith(':');
    const right = c.endsWith(':');
    aligns.push(left && right ? 'center' : right ? 'right' : left ? 'left' : null);
  }
  return aligns;
}

export function analyserMarkdown(texte: string): MdNode[] {
  const nodes: MdNode[] = [];
  const n = texte.length;
  let para = '';
  const flushPara = (): void => {
    if (para !== '') {
      nodes.push(...analyserFragment(para, 0));
      para = '';
    }
  };

  let i = 0;
  while (i < n) {
    const end = finDeLigne(texte, i);
    const line = texte.slice(i, end);

    // `>>> ` quotes the whole remainder of the message.
    if (texte.startsWith('>>> ', i)) {
      flushPara();
      nodes.push({
        type: 'blockquote',
        children: analyserFragment(texte.slice(i + 4), 0),
      });
      return nodes;
    }

    // `> line` quote — consecutive quoted lines merge into one block.
    if (line.startsWith('> ') || line === '>') {
      flushPara();
      const quoted: string[] = [];
      let j = i;
      while (j < n) {
        const qEnd = finDeLigne(texte, j);
        const qLine = texte.slice(j, qEnd);
        if (qLine.startsWith('> ')) quoted.push(qLine.slice(2));
        else if (qLine === '>') quoted.push('');
        else break;
        j = qEnd < n ? qEnd + 1 : n;
      }
      nodes.push({
        type: 'blockquote',
        children: analyserFragment(quoted.join('\n'), 0),
      });
      i = j;
      continue;
    }

    // Headings `# ` / `## ` / `### ` (line-start only, non-empty content).
    const heading = HEADING_RE.exec(line);
    const hashes = heading?.[1];
    const title = heading?.[2];
    if (hashes !== undefined && title !== undefined && title.trim() !== '') {
      flushPara();
      const level = hashes.length === 1 ? 1 : hashes.length === 2 ? 2 : 3;
      nodes.push({ type: 'heading', level, children: analyserFragment(title.trim(), 0) });
      i = end < n ? end + 1 : n;
      continue;
    }

    // Lists — consecutive item lines; a leading space nests one level.
    if (lireItem(line) !== null) {
      flushPara();
      const items: RawItem[] = [];
      let j = i;
      while (j < n) {
        const iEnd = finDeLigne(texte, j);
        const item = lireItem(texte.slice(j, iEnd));
        if (item === null) break;
        items.push(item);
        j = iEnd < n ? iEnd + 1 : n;
      }
      nodes.push(construireListe(items));
      i = j;
      continue;
    }

    // GFM tables : ligne d'en-tête suivie d'un séparateur d'alignements de
    // même largeur. Toute autre ligne contenant `|` retombe en paragraphe.
    if (line.includes('|')) {
      const nextStart = end < n ? end + 1 : n;
      if (nextStart < n) {
        const nextEnd = finDeLigne(texte, nextStart);
        const aligns = lireAlignements(texte.slice(nextStart, nextEnd));
        const header = decouperCellules(line);
        if (aligns !== null && aligns.length === header.length) {
          flushPara();
          const rows: MdNode[][][] = [];
          let j = nextEnd < n ? nextEnd + 1 : n;
          while (j < n) {
            const rEnd = finDeLigne(texte, j);
            const rLine = texte.slice(j, rEnd);
            if (rLine.trim() === '' || !rLine.includes('|')) break;
            const cells = decouperCellules(rLine);
            rows.push(header.map((_, c) => analyserFragment(cells[c] ?? '', 0)));
            j = rEnd < n ? rEnd + 1 : n;
          }
          nodes.push({
            type: 'table',
            align: aligns,
            header: header.map((h) => analyserFragment(h, 0)),
            rows,
          });
          i = j;
          continue;
        }
      }
    }

    // Paragraph line (may extend past `\n` when a code fence spans lines).
    const paraEnd = finDeParagraphe(texte, i, end);
    para += (para === '' ? '' : '\n') + texte.slice(i, paraEnd);
    i = paraEnd < n ? paraEnd + 1 : n;
  }

  flushPara();
  return nodes;
}
