/**
 * Mise en forme Markdown du journal d'audit d'un serveur (feuille de route
 * §9.4, « journal d'audit exportable »).
 *
 * Fonction PURE, sans dépendance à l'API ni à React : elle reçoit des lignes
 * déjà résolues — acteur nommé, action décrite, horodatage formaté — et rend
 * le texte. Même découpage que `transcript.ts`, et pour la même raison : ce
 * qu'il y a à éprouver ici, c'est le format, pas le chemin réseau.
 *
 * 🔒 **Ce que cet export contient, délibérément.** Les noms des membres et le
 * détail des actions de modération. C'est le contraire du rapport de
 * diagnostic (`diagnostics.report`), qui caviarde tout ce qui désigne
 * quelqu'un — et la différence n'est pas une incohérence : ce journal-ci est
 * le registre du serveur, destiné à ses propres modérateurs, et un registre
 * anonymisé ne sert à rien. Le rapport de bug, lui, part chez un inconnu.
 *
 * Ne jamais copier ce format vers quelque chose qui sortirait du cercle des
 * modérateurs sans reprendre cette question depuis le début.
 */

/** Une ligne du journal, déjà résolue et traduite par l'appelant. */
export interface AuditExportLine {
  /** Nom affiché de l'acteur (jamais sa clé publique brute).  */
  readonly actor: string;
  /** Description de l'action, dans la langue de l'interface. */
  readonly action: string;
  /** Horodatage déjà formaté. */
  readonly at: string;
}

/** Libellés d'encadrement, tous déjà traduits. */
export interface AuditExportLabels {
  readonly heading: string;
  /** Ligne de contexte : serveur, date d'export, nombre d'entrées. */
  readonly subtitle: string;
  readonly empty: string;
  readonly columnAt: string;
  readonly columnActor: string;
  readonly columnAction: string;
  /** Averti que l'export s'arrête aux `n` entrées les plus récentes. */
  readonly truncated: string | null;
}

/**
 * Échappe ce qui casserait une cellule de tableau Markdown.
 *
 * Un nom de serveur ou de salon peut contenir une barre verticale — c'est du
 * texte libre saisi par un utilisateur. Sans échappement, une seule barre
 * décale toutes les colonnes de la ligne, et le registre devient illisible
 * exactement là où quelqu'un a voulu qu'il le soit.
 */
function cellule(texte: string): string {
  return texte.replace(/\|/g, '\\|').replace(/\r?\n/g, ' ');
}

/**
 * Rend le journal en Markdown : un tableau, du plus récent au plus ancien
 * (l'ordre dans lequel l'onglet l'affiche).
 */
export function buildAuditExport(
  lines: readonly AuditExportLine[],
  labels: AuditExportLabels,
): string {
  const parts = [`# ${labels.heading}`, '', labels.subtitle, ''];

  if (lines.length === 0) {
    parts.push(labels.empty, '');
    return parts.join('\n');
  }

  parts.push(
    `| ${labels.columnAt} | ${labels.columnActor} | ${labels.columnAction} |`,
    '| --- | --- | --- |',
  );
  for (const l of lines) {
    parts.push(`| ${cellule(l.at)} | ${cellule(l.actor)} | ${cellule(l.action)} |`);
  }
  parts.push('');
  if (labels.truncated !== null) {
    parts.push(labels.truncated, '');
  }
  return parts.join('\n');
}
