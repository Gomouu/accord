/**
 * Vérification d'identité par QR (§17.4) : charge utile du QR d'un numéro de
 * sécurité, relecture d'un QR scanné, verdict de comparaison.
 *
 * La cérémonie existante consiste à se lire 60 chiffres à voix haute ; le QR
 * ne la remplace pas, il en supprime la principale cause d'échec (l'erreur de
 * transcription). Un ami affiche le QR de *son* numéro, l'autre le scanne, et
 * l'application compare — les chiffres restent affichés dans les deux cas.
 *
 * 🔒 **Rien de ce qui sort d'un QR n'est adopté.** La charge utile ne
 * transporte qu'une valeur à comparer : jamais une clé, jamais une identité,
 * jamais quoi que ce soit qu'on stocke ou qu'on affiche comme étant celle du
 * pair. La comparaison se fait toujours contre le numéro calculé localement
 * (`friends.safety_number`, dérivé des deux clés publiques déjà connues). Un
 * QR ne peut donc que confirmer ce que l'appareil savait déjà, ou le
 * contredire — il ne peut rien lui apprendre.
 *
 * 🔒 **Un échec de décodage n'est jamais un succès.** Les trois portes de
 * sortie sont distinctes et une seule vaut « identique » : elle exige à la
 * fois une charge utile bien formée *et* l'égalité stricte avec les chiffres
 * locaux. Voir [`verdictForScan`].
 */

/**
 * Schéma de la charge utile. Distinct de `accord://friend/<code>` : un lien
 * d'ami scanné ici doit être rejeté comme étranger, pas confondu avec un
 * numéro de sécurité (et réciproquement).
 */
const SAFETY_QR_PREFIX = 'accord://safety/';

/** Longueur d'un numéro de sécurité : 60 chiffres (12 groupes de 5). */
export const SAFETY_DIGITS_LENGTH = 60;

/**
 * Charge utile acceptable : le préfixe, puis exactement 60 chiffres ASCII,
 * et rien d'autre. `\d` est bien la classe ASCII `[0-9]` en JavaScript — un
 * chiffre arabo-indien ne passe donc pas pour un chiffre décimal.
 */
const SAFETY_QR_PATTERN = new RegExp(
  `^${SAFETY_QR_PREFIX.replace(/[/:.]/g, '\\$&')}(\\d{${SAFETY_DIGITS_LENGTH}})$`,
);

/**
 * Charge utile du QR à afficher pour `digits`. Le numéro de sécurité n'est pas
 * un secret : les deux pairs le calculent chacun de leur côté, et il ne dit
 * rien de plus que les clés publiques dont il dérive.
 */
export function buildSafetyQrPayload(digits: string): string {
  return `${SAFETY_QR_PREFIX}${digits}`;
}

/**
 * Chiffres portés par un QR scanné, ou `null` si le texte décodé n'est pas une
 * charge utile de numéro de sécurité (autre QR, lien d'ami, texte libre,
 * longueur inattendue). Ne rend jamais une chaîne vide : la valeur rendue fait
 * toujours exactement [`SAFETY_DIGITS_LENGTH`] chiffres.
 */
export function parseSafetyQrPayload(text: string): string | null {
  return SAFETY_QR_PATTERN.exec(text.trim())?.[1] ?? null;
}

/**
 * Issue d'un scan confronté au numéro local.
 *
 * - `match` — même numéro : la conversation est authentifiée.
 * - `mismatch` — numéro différent : à dire tel quel, sans adoucissement.
 * - `foreign` — le QR a été lu mais ne porte pas de numéro de sécurité ; rien
 *   n'a été comparé, donc rien n'est conclu.
 */
export type SafetyScanVerdict = 'match' | 'mismatch' | 'foreign';

/**
 * Verdict d'une image analysée. `decoded` vaut `null` quand aucun QR n'a été
 * trouvé dans l'image — on rend alors `null` pour continuer à scanner, ce qui
 * n'est pas un verdict et ne doit jamais être présenté comme tel.
 *
 * 🔒 `match` a une seule origine dans tout le fichier : `parseSafetyQrPayload`
 * a rendu 60 chiffres *et* ils sont strictement égaux aux chiffres locaux. Une
 * lecture ratée retombe sur `null` ou `foreign`, jamais sur `match`.
 */
export function verdictForScan(
  decoded: string | null,
  localDigits: string,
): SafetyScanVerdict | null {
  if (decoded === null) return null;
  const scanned = parseSafetyQrPayload(decoded);
  if (scanned === null) return 'foreign';
  return scanned === localDigits ? 'match' : 'mismatch';
}
