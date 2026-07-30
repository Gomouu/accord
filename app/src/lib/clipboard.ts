/**
 * Durée d'affichage du retour « Copié ! » d'un bouton de copie, en
 * millisecondes.
 *
 * Vit ici plutôt que dans chaque écran : la valeur était recopiée à
 * l'identique dans quatre composants, si bien que la régler revenait à
 * retrouver les quatre.
 */
export const COPY_FEEDBACK_MS = 1500;

/**
 * Copie presse-papiers en best effort : l'API `navigator.clipboard` peut être
 * absente ou refusée (environnement restreint) — l'échec est silencieux côté
 * exception, signalé à l'appelant via `onError` pour un retour utilisateur
 * (toast). Même garde que l'ancien `copyLink` de `MessageList`, généralisée
 * pour tous les usages « Copier … » du menu contextuel.
 *
 * ⚠️ Passer par cette fonction plutôt que d'appeler `navigator.clipboard`
 * directement : un `writeText().then(…)` sans `.catch` laisse un rejet de
 * promesse non géré à chaque refus du presse-papiers, et l'utilisateur ne voit
 * alors ni confirmation ni erreur.
 */
export function copyToClipboard(
  text: string,
  onSuccess: () => void,
  onError: () => void,
): void {
  try {
    void navigator.clipboard.writeText(text).then(onSuccess).catch(onError);
  } catch {
    onError();
  }
}
