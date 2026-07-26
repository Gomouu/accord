import { describe, expect, it } from 'vitest';
import { buildAuditExport, type AuditExportLabels } from './auditExport';

const LABELS: AuditExportLabels = {
  heading: "Journal d'audit — Atelier Cipher",
  subtitle: 'Exporté le 26/07/2026 · 2 entrées',
  empty: 'Aucune action enregistrée.',
  columnAt: 'Date',
  columnActor: 'Auteur',
  columnAction: 'Action',
  truncated: null,
};

describe('buildAuditExport', () => {
  it('rend un tableau, du plus récent au plus ancien', () => {
    const md = buildAuditExport(
      [
        { at: '26/07 14:02', actor: 'Alice', action: 'a banni Mallory' },
        { at: '26/07 13:58', actor: 'Bob', action: 'a créé #général' },
      ],
      LABELS,
    );

    expect(md).toContain("# Journal d'audit — Atelier Cipher");
    expect(md).toContain('| Date | Auteur | Action |');
    expect(md).toContain('| 26/07 14:02 | Alice | a banni Mallory |');
    // L'ordre reçu est conservé : c'est celui que l'onglet affiche.
    expect(md.indexOf('Alice')).toBeLessThan(md.indexOf('Bob'));
  });

  it('échappe une barre verticale dans un nom saisi par un utilisateur', () => {
    // 🔒 Le cas qui compte : un nom de salon ou de membre est du texte libre.
    // Une seule barre non échappée décale toutes les colonnes de la ligne, et
    // rend le registre illisible précisément là où quelqu'un l'a voulu.
    const md = buildAuditExport(
      [{ at: '26/07 14:02', actor: 'Ma|lory', action: 'a créé #a | b' }],
      LABELS,
    );

    const ligne = md.split('\n').find((l) => l.includes('Ma'));
    expect(ligne).toBe('| 26/07 14:02 | Ma\\|lory | a créé #a \\| b |');
    // Trois séparateurs de colonnes, pas cinq.
    expect(ligne?.split(/(?<!\\)\|/).length).toBe(5);
  });

  it('réduit un saut de ligne à une espace', () => {
    const md = buildAuditExport(
      [{ at: '26/07', actor: 'Alice', action: 'a renommé en\nDeux lignes' }],
      LABELS,
    );
    expect(md).toContain('| 26/07 | Alice | a renommé en Deux lignes |');
  });

  it('rend un journal vide sans tableau', () => {
    const md = buildAuditExport([], LABELS);
    expect(md).toContain('Aucune action enregistrée.');
    expect(md).not.toContain('| --- |');
  });

  it('avertit quand l’export est tronqué', () => {
    // Un registre incomplet qui ne le dit pas est pire qu'un registre court :
    // le lecteur conclut qu'il n'y a rien eu avant.
    const md = buildAuditExport([{ at: 'x', actor: 'y', action: 'z' }], {
      ...LABELS,
      truncated: 'Limité aux 500 actions les plus récentes.',
    });
    expect(md).toContain('Limité aux 500 actions les plus récentes.');
  });
});
