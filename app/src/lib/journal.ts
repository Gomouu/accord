/**
 * Journalisation de la webview vers le journal du nœud (feuille de route
 * §10.6, point 2).
 *
 * **Pourquoi.** La moitié de l'application est du TypeScript dans une webview.
 * Sa console n'existe pas en production : une erreur de rendu ou un rejet de
 * promesse ne laissait aucune trace, et c'est précisément le genre de panne
 * qu'on cherche à comprendre après coup. Ces lignes partent maintenant dans le
 * MÊME fichier que le nœud — un seul fichier, une seule horloge, parce que
 * deux journaux qu'il faut recoller à la main n'aident personne à lire un
 * enchaînement où l'interface et le réseau se répondent.
 *
 * 🔒 **Ce qui ne doit jamais passer par ici** : contenu de message, clé, code
 * ami, adresse d'un ami. Le journal existe pour être envoyé à quelqu'un ; ce
 * qui y entre en sort. Le nœud ne filtre pas — il ne le peut pas, une chaîne
 * déjà composée ne se relit pas — donc la responsabilité est à l'appel.
 */

import { invoke } from '@tauri-apps/api/core';

/** Niveaux acceptés par la commande `journal_ui` du nœud. */
export type NiveauJournal = 'error' | 'warn' | 'info' | 'debug';

/**
 * Écrit une ligne dans le journal du nœud.
 *
 * Silencieux en cas d'échec, et volontairement : journaliser est une action
 * secondaire. Faire remonter l'échec d'une écriture de journal transformerait
 * un incident en deux, et le second masquerait le premier. Hors Tauri (tests,
 * navigateur), `invoke` échoue et la ligne est simplement perdue.
 */
export function journaliser(niveau: NiveauJournal, message: string): void {
  void invoke('journal_ui', { niveau, message }).catch(() => {});
}

/**
 * Branche les deux pièges globaux du navigateur.
 *
 * `unhandledrejection` d'abord : c'est lui qui a manqué le plus cruellement.
 * Un `await` sans `catch` dans un composant échoue en silence complet — ni
 * écran rouge, ni console visible, rien.
 *
 * Rend une fonction de retrait, pour que les tests n'accumulent pas des
 * écouteurs d'une exécution à l'autre.
 */
export function installerPiegesGlobaux(): () => void {
  const surRejet = (e: PromiseRejectionEvent): void => {
    journaliser('error', `rejet de promesse non traité : ${decrire(e.reason)}`);
  };
  const surErreur = (e: ErrorEvent): void => {
    // `e.message` peut être vide sur une erreur inter-origine ; la source et
    // la ligne restent utiles pour situer.
    journaliser(
      'error',
      `erreur non traitée : ${e.message || '(sans message)'} @ ${e.filename}:${e.lineno}`,
    );
  };
  window.addEventListener('unhandledrejection', surRejet);
  window.addEventListener('error', surErreur);
  return () => {
    window.removeEventListener('unhandledrejection', surRejet);
    window.removeEventListener('error', surErreur);
  };
}

/**
 * Réduit une valeur rejetée à une ligne lisible.
 *
 * Une promesse peut être rejetée avec n'importe quoi — une `Error`, une
 * chaîne, un objet, `undefined`. Sans cette normalisation, le journal reçoit
 * « [object Object] », qui ne dit rien de plus que l'absence de ligne.
 */
function decrire(raison: unknown): string {
  if (raison instanceof Error) {
    return `${raison.name}: ${raison.message}`;
  }
  if (typeof raison === 'string') return raison;
  try {
    return JSON.stringify(raison) ?? String(raison);
  } catch {
    return String(raison);
  }
}
