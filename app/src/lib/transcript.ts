/**
 * Assemble une transcription Markdown d'une conversation (MP ou salon) — pur,
 * sans locale ni React : l'appelant fournit les libellés déjà traduits et les
 * fonctions de rendu de date/heure/nom (`TranscriptFormatters`), ce module ne
 * fait qu'agencer les chaînes. Permet « Copier la conversation » sans nouvelle
 * surface d'écriture disque, et reste trivialement testable.
 */

/** Un message aplati pour l'export ; `text` est déjà résolu (édition, média). */
export interface TranscriptMessage {
  readonly author: string;
  readonly sentMs: number;
  readonly deleted: boolean;
  /** Contenu lisible (corps ou placeholder média) ; `null` = aucun texte. */
  readonly text: string | null;
  readonly edited: boolean;
  /** Noms de fichiers des pièces jointes (vide/absent si aucune). */
  readonly attachments?: readonly string[];
}

/** Libellés traduits injectés par l'appelant (aucun i18n ici). */
export interface TranscriptLabels {
  readonly heading: string;
  readonly subtitle: string;
  readonly deleted: string;
  readonly attachment: string;
  readonly edited: string;
  readonly empty: string;
}

/** Fonctions de rendu injectées (nom affiché, jour, heure). */
export interface TranscriptFormatters {
  readonly nameOf: (author: string) => string;
  readonly dayOf: (ms: number) => string;
  readonly timeOf: (ms: number) => string;
}

/** Corps Markdown d'un message unique (placeholder si supprimé/vide). */
function corps(msg: TranscriptMessage, labels: TranscriptLabels): string {
  if (msg.deleted) return `_${labels.deleted}_`;
  const parts: string[] = [];
  const texte = msg.text?.trim();
  if (texte !== undefined && texte !== '') parts.push(texte);
  for (const nom of msg.attachments ?? []) {
    parts.push(`📎 ${labels.attachment} : ${nom}`);
  }
  if (parts.length === 0) parts.push(`_${labels.empty}_`);
  const corpsTexte = parts.join('\n');
  return msg.edited ? `${corpsTexte} _(${labels.edited})_` : corpsTexte;
}

/**
 * Transcription Markdown complète : titre, sous-titre, puis les messages
 * groupés par jour (`## <jour>`), chacun préfixé de `**<nom>** · <heure>`.
 * Une liste vide rend le titre suivi du libellé « aucun message ».
 */
export function buildTranscript(
  messages: readonly TranscriptMessage[],
  labels: TranscriptLabels,
  fmt: TranscriptFormatters,
): string {
  const lignes: string[] = [`# ${labels.heading}`, '', `_${labels.subtitle}_`];
  if (messages.length === 0) {
    lignes.push('', `_${labels.empty}_`);
    return `${lignes.join('\n')}\n`;
  }
  let jourCourant: string | null = null;
  for (const msg of messages) {
    const jour = fmt.dayOf(msg.sentMs);
    if (jour !== jourCourant) {
      lignes.push('', `## ${jour}`);
      jourCourant = jour;
    }
    lignes.push('', `**${fmt.nameOf(msg.author)}** · ${fmt.timeOf(msg.sentMs)}`);
    lignes.push(corps(msg, labels));
  }
  return `${lignes.join('\n')}\n`;
}
