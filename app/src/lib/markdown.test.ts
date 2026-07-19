/**
 * Tests du markdown léger PUR : chaque marque (gras, italique, barré, code,
 * bloc, spoiler, lien, mention, émoji), imbrication, échappement, gardes de
 * limite de mot et cas dégénérés (délimiteurs non fermés).
 *
 * Discord-level constructs: headings, lists (one nesting level), quotes,
 * underline, masked links and fenced code blocks with a language tag.
 */

import { describe, expect, it } from 'vitest';
import { analyserMarkdown, type MdNode } from './markdown';

const text = (value: string): MdNode => ({ type: 'text', value });

describe('analyserMarkdown — texte et sauts de ligne', () => {
  it('rend le texte brut tel quel', () => {
    expect(analyserMarkdown('bonjour à tous')).toEqual([text('bonjour à tous')]);
  });

  it('découpe les sauts de ligne en nœuds break', () => {
    expect(analyserMarkdown('a\nb')).toEqual([text('a'), { type: 'break' }, text('b')]);
  });

  it('rend une chaîne vide sans nœud', () => {
    expect(analyserMarkdown('')).toEqual([]);
  });
});

describe('analyserMarkdown — mise en forme', () => {
  it('gras **…**', () => {
    expect(analyserMarkdown('**gras**')).toEqual([
      { type: 'bold', children: [text('gras')] },
    ]);
  });

  it('italique *…*', () => {
    expect(analyserMarkdown('*ital*')).toEqual([
      { type: 'italic', children: [text('ital')] },
    ]);
  });

  it('italique _…_ aux bords de mot', () => {
    expect(analyserMarkdown('_ital_')).toEqual([
      { type: 'italic', children: [text('ital')] },
    ]);
  });

  it('ne coupe pas les underscores au milieu d’un mot (snake_case)', () => {
    expect(analyserMarkdown('snake_case')).toEqual([text('snake_case')]);
  });

  it('barré ~~…~~', () => {
    expect(analyserMarkdown('~~barré~~')).toEqual([
      { type: 'strike', children: [text('barré')] },
    ]);
  });

  it('spoiler ||…||', () => {
    expect(analyserMarkdown('||secret||')).toEqual([
      { type: 'spoiler', children: [text('secret')] },
    ]);
  });

  it('imbrique italique dans gras', () => {
    expect(analyserMarkdown('**gras _ital_**')).toEqual([
      {
        type: 'bold',
        children: [text('gras '), { type: 'italic', children: [text('ital')] }],
      },
    ]);
  });
});

describe('analyserMarkdown — code (littéral)', () => {
  it('code inline `…` sans sous-mise-en-forme', () => {
    expect(analyserMarkdown('`**x**`')).toEqual([{ type: 'code', value: '**x**' }]);
  });

  it('bloc de code ```…``` avec sauts de ligne', () => {
    expect(analyserMarkdown('```\nhello\nworld\n```')).toEqual([
      { type: 'codeblock', value: 'hello\nworld' },
    ]);
  });

  it('bloc de code sur une ligne', () => {
    expect(analyserMarkdown('```code```')).toEqual([
      { type: 'codeblock', value: 'code' },
    ]);
  });
});

describe('analyserMarkdown — liens', () => {
  it('détecte une URL https', () => {
    expect(analyserMarkdown('https://exemple.fr')).toEqual([
      { type: 'link', href: 'https://exemple.fr', value: 'https://exemple.fr' },
    ]);
  });

  it('retire la ponctuation finale de l’URL', () => {
    expect(analyserMarkdown('voir https://ex.fr.')).toEqual([
      text('voir '),
      { type: 'link', href: 'https://ex.fr', value: 'https://ex.fr' },
      text('.'),
    ]);
  });

  it('ne transforme pas un schéma non http(s)', () => {
    expect(analyserMarkdown('ftp://ex.fr')).toEqual([text('ftp://ex.fr')]);
  });
});

describe('analyserMarkdown — mentions et émojis', () => {
  it('mention @pseudo', () => {
    expect(analyserMarkdown('salut @bob !')).toEqual([
      text('salut '),
      { type: 'mention', name: 'bob' },
      text(' !'),
    ]);
  });

  it('émoji custom :name:', () => {
    expect(analyserMarkdown(':parrot:')).toEqual([{ type: 'emoji', name: 'parrot' }]);
  });

  it('ignore un nom d’émoji trop court (min 2)', () => {
    expect(analyserMarkdown(':x:')).toEqual([text(':x:')]);
  });

  it('compose émoji, gras et mention dans le même texte', () => {
    expect(analyserMarkdown('**hey** @bob :wave:')).toEqual([
      { type: 'bold', children: [text('hey')] },
      text(' '),
      { type: 'mention', name: 'bob' },
      text(' '),
      { type: 'emoji', name: 'wave' },
    ]);
  });
});

describe('analyserMarkdown — underline', () => {
  it('parses __…__ as underline', () => {
    expect(analyserMarkdown('__sous__')).toEqual([
      { type: 'underline', children: [text('sous')] },
    ]);
  });

  it('keeps ** bold and __ underline distinct in one message', () => {
    expect(analyserMarkdown('**b** et __u__')).toEqual([
      { type: 'bold', children: [text('b')] },
      text(' et '),
      { type: 'underline', children: [text('u')] },
    ]);
  });

  it('parses bold nested inside underline', () => {
    expect(analyserMarkdown('__a **b**__')).toEqual([
      {
        type: 'underline',
        children: [text('a '), { type: 'bold', children: [text('b')] }],
      },
    ]);
  });

  it('leaves an unclosed __ literal', () => {
    expect(analyserMarkdown('__pas fermé')).toEqual([text('__pas fermé')]);
  });

  it('still underlines dunder-style words (__init__)', () => {
    expect(analyserMarkdown('__init__')).toEqual([
      { type: 'underline', children: [text('init')] },
    ]);
  });
});

describe('analyserMarkdown — headings', () => {
  it('parses # / ## / ### at line start', () => {
    expect(analyserMarkdown('# Un')).toEqual([
      { type: 'heading', level: 1, children: [text('Un')] },
    ]);
    expect(analyserMarkdown('## Deux')).toEqual([
      { type: 'heading', level: 2, children: [text('Deux')] },
    ]);
    expect(analyserMarkdown('### Trois')).toEqual([
      { type: 'heading', level: 3, children: [text('Trois')] },
    ]);
  });

  it('requires a space after the hashes', () => {
    expect(analyserMarkdown('#pas-titre')).toEqual([text('#pas-titre')]);
  });

  it('ignores four or more hashes', () => {
    expect(analyserMarkdown('#### quatre')).toEqual([text('#### quatre')]);
  });

  it('ignores a hash that is not at line start', () => {
    expect(analyserMarkdown('a # b')).toEqual([text('a # b')]);
  });

  it('ignores an empty heading', () => {
    expect(analyserMarkdown('# ')).toEqual([text('# ')]);
  });

  it('splits paragraph / heading / paragraph on separate lines', () => {
    expect(analyserMarkdown('a\n# T\nb')).toEqual([
      text('a'),
      { type: 'heading', level: 1, children: [text('T')] },
      text('b'),
    ]);
  });

  it('parses inline formatting and mentions inside a heading', () => {
    expect(analyserMarkdown('# salut **@bob**')).toEqual([
      {
        type: 'heading',
        level: 1,
        children: [
          text('salut '),
          { type: 'bold', children: [{ type: 'mention', name: 'bob' }] },
        ],
      },
    ]);
  });

  it('does not parse a heading inside a code fence', () => {
    expect(analyserMarkdown('```\n# pas un titre\n```')).toEqual([
      { type: 'codeblock', value: '# pas un titre' },
    ]);
  });
});

describe('analyserMarkdown — lists', () => {
  it('parses an unordered list with - and *', () => {
    expect(analyserMarkdown('- a\n* b')).toEqual([
      { type: 'list', ordered: false, start: 1, items: [[text('a')], [text('b')]] },
    ]);
  });

  it('parses an ordered list and keeps its start number', () => {
    expect(analyserMarkdown('3. a\n4. b')).toEqual([
      { type: 'list', ordered: true, start: 3, items: [[text('a')], [text('b')]] },
    ]);
  });

  it('requires a space after the marker', () => {
    expect(analyserMarkdown('-pas une liste')).toEqual([text('-pas une liste')]);
    expect(analyserMarkdown('1.pas une liste')).toEqual([text('1.pas une liste')]);
  });

  it('nests indented items one level under the previous item', () => {
    expect(analyserMarkdown('- a\n - a1\n - a2\n- b')).toEqual([
      {
        type: 'list',
        ordered: false,
        start: 1,
        items: [
          [
            text('a'),
            {
              type: 'list',
              ordered: false,
              start: 1,
              items: [[text('a1')], [text('a2')]],
            },
          ],
          [text('b')],
        ],
      },
    ]);
  });

  it('treats an indented item with no parent as top-level', () => {
    expect(analyserMarkdown(' - seul')).toEqual([
      { type: 'list', ordered: false, start: 1, items: [[text('seul')]] },
    ]);
  });

  it('ends the list at the first non-item line', () => {
    expect(analyserMarkdown('- a\nsuite')).toEqual([
      { type: 'list', ordered: false, start: 1, items: [[text('a')]] },
      text('suite'),
    ]);
  });

  it('parses emoji and mentions inside list items', () => {
    expect(analyserMarkdown('- salut @bob :wave:')).toEqual([
      {
        type: 'list',
        ordered: false,
        start: 1,
        items: [
          [
            text('salut '),
            { type: 'mention', name: 'bob' },
            text(' '),
            { type: 'emoji', name: 'wave' },
          ],
        ],
      },
    ]);
  });

  it('keeps the type of the first marker for mixed lists', () => {
    expect(analyserMarkdown('1. a\n- b')).toEqual([
      { type: 'list', ordered: true, start: 1, items: [[text('a')], [text('b')]] },
    ]);
  });
});

describe('analyserMarkdown — blockquotes', () => {
  it('parses a single quoted line', () => {
    expect(analyserMarkdown('> citation')).toEqual([
      { type: 'blockquote', children: [text('citation')] },
    ]);
  });

  it('merges consecutive quoted lines into one block', () => {
    expect(analyserMarkdown('> a\n> b')).toEqual([
      { type: 'blockquote', children: [text('a'), { type: 'break' }, text('b')] },
    ]);
  });

  it('quotes the whole remainder with >>>', () => {
    expect(analyserMarkdown('>>> a\nb')).toEqual([
      { type: 'blockquote', children: [text('a'), { type: 'break' }, text('b')] },
    ]);
  });

  it('requires a space after >', () => {
    expect(analyserMarkdown('>pas cité')).toEqual([text('>pas cité')]);
  });

  it('stops the quote at the first unquoted line', () => {
    expect(analyserMarkdown('> a\nsuite')).toEqual([
      { type: 'blockquote', children: [text('a')] },
      text('suite'),
    ]);
  });

  it('parses formatting and emoji inside a quote', () => {
    expect(analyserMarkdown('> **b** :wave:')).toEqual([
      {
        type: 'blockquote',
        children: [
          { type: 'bold', children: [text('b')] },
          text(' '),
          { type: 'emoji', name: 'wave' },
        ],
      },
    ]);
  });
});

describe('analyserMarkdown — masked links', () => {
  it('parses [label](https://url)', () => {
    expect(analyserMarkdown('[docs](https://ex.fr/doc)')).toEqual([
      { type: 'masklink', href: 'https://ex.fr/doc', children: [text('docs')] },
    ]);
  });

  it('parses formatting inside the label', () => {
    expect(analyserMarkdown('[**gras**](https://ex.fr)')).toEqual([
      {
        type: 'masklink',
        href: 'https://ex.fr',
        children: [{ type: 'bold', children: [text('gras')] }],
      },
    ]);
  });

  it('rejects non-http(s) schemes as literal text', () => {
    expect(analyserMarkdown('[x](javascript:alert(1))')).toEqual([
      text('[x](javascript:alert(1))'),
    ]);
    expect(analyserMarkdown('[x](ftp://ex.fr)')).toEqual([text('[x](ftp://ex.fr)')]);
  });

  it('keeps a malformed masked link literal (URL still auto-linked)', () => {
    expect(analyserMarkdown('[label](https://ex.fr')).toEqual([
      text('[label]('),
      { type: 'link', href: 'https://ex.fr', value: 'https://ex.fr' },
    ]);
    expect(analyserMarkdown('[label] (https://ex.fr)')).toEqual([
      text('[label] ('),
      { type: 'link', href: 'https://ex.fr', value: 'https://ex.fr' },
      text(')'),
    ]);
  });

  it('keeps an empty label literal', () => {
    expect(analyserMarkdown('[](https://ex.fr)')).toEqual([
      text('[]('),
      { type: 'link', href: 'https://ex.fr', value: 'https://ex.fr' },
      text(')'),
    ]);
  });

  it('keeps auto-linking working next to masked links', () => {
    expect(analyserMarkdown('[a](https://a.fr) https://b.fr')).toEqual([
      { type: 'masklink', href: 'https://a.fr', children: [text('a')] },
      text(' '),
      { type: 'link', href: 'https://b.fr', value: 'https://b.fr' },
    ]);
  });
});

describe('analyserMarkdown — code fences with language', () => {
  it('extracts the language tag from the opening fence', () => {
    expect(analyserMarkdown('```js\nconst a = 1;\n```')).toEqual([
      { type: 'codeblock', value: 'const a = 1;', lang: 'js' },
    ]);
  });

  it('treats a fence without newline after the tag as content', () => {
    expect(analyserMarkdown('```code```')).toEqual([
      { type: 'codeblock', value: 'code' },
    ]);
  });

  it('keeps block markers literal inside a fenced block', () => {
    expect(analyserMarkdown('```py\n# comment\n- item\n> quote\n```')).toEqual([
      { type: 'codeblock', value: '# comment\n- item\n> quote', lang: 'py' },
    ]);
  });

  it('resumes block parsing after a fence', () => {
    expect(analyserMarkdown('```js\nx\n```\n# T')).toEqual([
      { type: 'codeblock', value: 'x', lang: 'js' },
      { type: 'heading', level: 1, children: [text('T')] },
    ]);
  });

  it('consumes a fence opened mid-line across lines', () => {
    expect(analyserMarkdown('a ```js\n# pas un titre\n``` b')).toEqual([
      text('a '),
      { type: 'codeblock', value: '# pas un titre', lang: 'js' },
      text(' b'),
    ]);
  });
});

describe('analyserMarkdown — échappement et cas dégénérés', () => {
  it('l’antislash rend le caractère suivant littéral', () => {
    expect(analyserMarkdown('\\*pas gras\\*')).toEqual([text('*pas gras*')]);
  });

  it('un délimiteur non fermé reste littéral', () => {
    expect(analyserMarkdown('**pas fermé')).toEqual([text('**pas fermé')]);
  });

  it('ignore un délimiteur de fermeture échappé', () => {
    // Le premier `*` d'ouverture ne trouve pas de fermeture non échappée.
    expect(analyserMarkdown('*a\\*b')).toEqual([text('*a*b')]);
  });
});

describe('analyserMarkdown — tables GFM', () => {
  it('rend une table simple avec en-tête et lignes', () => {
    const md = '| A | B |\n| --- | --- |\n| 1 | 2 |\n| 3 | 4 |';
    expect(analyserMarkdown(md)).toEqual([
      {
        type: 'table',
        align: [null, null],
        header: [[text('A')], [text('B')]],
        rows: [
          [[text('1')], [text('2')]],
          [[text('3')], [text('4')]],
        ],
      },
    ]);
  });

  it('interprète les alignements gauche/centre/droite', () => {
    const md = '| a | b | c |\n| :-- | :--: | --: |\n| x | y | z |';
    const nodes = analyserMarkdown(md);
    expect(nodes).toHaveLength(1);
    const table = nodes[0] as Extract<MdNode, { type: 'table' }>;
    expect(table.align).toEqual(['left', 'center', 'right']);
  });

  it('applique la mise en forme inline dans les cellules', () => {
    const md = '| titre |\n| --- |\n| **gras** |';
    const table = analyserMarkdown(md)[0] as Extract<MdNode, { type: 'table' }>;
    expect(table.rows[0]?.[0]).toEqual([{ type: 'bold', children: [text('gras')] }]);
  });

  it('complète les cellules manquantes d’une ligne courte', () => {
    const md = '| A | B |\n| --- | --- |\n| seul |';
    const table = analyserMarkdown(md)[0] as Extract<MdNode, { type: 'table' }>;
    expect(table.rows[0]).toEqual([[text('seul')], []]);
  });

  it('ne confond pas un paragraphe contenant un tube avec une table', () => {
    const nodes = analyserMarkdown('a | b\nsuite');
    expect(nodes.some((nd) => nd.type === 'table')).toBe(false);
  });

  it('exige un séparateur de même largeur que l’en-tête', () => {
    const nodes = analyserMarkdown('| A | B |\n| --- |\n| 1 | 2 |');
    expect(nodes.some((nd) => nd.type === 'table')).toBe(false);
  });

  it('gère les tubes échappés dans une cellule', () => {
    const md = '| A |\n| --- |\n| x \\| y |';
    const table = analyserMarkdown(md)[0] as Extract<MdNode, { type: 'table' }>;
    expect(table.rows[0]?.[0]).toEqual([text('x | y')]);
  });
});

describe('analyserMarkdown — listes de tâches GFM', () => {
  it('extrait une case cochée / décochée en tête d’item', () => {
    const nodes = analyserMarkdown('- [ ] à faire\n- [x] fait');
    expect(nodes).toHaveLength(1);
    const list = nodes[0] as Extract<MdNode, { type: 'list' }>;
    expect(list.items[0]?.[0]).toEqual({ type: 'checkbox', checked: false });
    expect(list.items[0]?.[1]).toEqual(text('à faire'));
    expect(list.items[1]?.[0]).toEqual({ type: 'checkbox', checked: true });
  });

  it('accepte [X] majuscule', () => {
    const list = analyserMarkdown('- [X] ok')[0] as Extract<MdNode, { type: 'list' }>;
    expect(list.items[0]?.[0]).toEqual({ type: 'checkbox', checked: true });
  });

  it('applique la mise en forme inline après la case', () => {
    const list = analyserMarkdown('- [ ] **gras**')[0] as Extract<
      MdNode,
      { type: 'list' }
    >;
    expect(list.items[0]?.[1]).toEqual({ type: 'bold', children: [text('gras')] });
  });

  it('ne touche pas un item de liste ordinaire', () => {
    const list = analyserMarkdown('- simple')[0] as Extract<MdNode, { type: 'list' }>;
    expect(list.items[0]?.[0]).toEqual(text('simple'));
  });
});
