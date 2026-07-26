/**
 * Préparation de l'environnement de test : étend `expect` de Vitest avec les
 * matchers DOM de Testing Library (`toBeInTheDocument`, etc.).
 */

import '@testing-library/jest-dom/vitest';

// 🔒 `localStorage` de test, indépendant de la version de Node.
//
// Node ≥ 22 définit lui-même un accesseur global `localStorage` qui rend
// `undefined` tant qu'on ne lui passe pas `--localstorage-file`. Cet accesseur
// masque celui de jsdom : sous Node 26, `window.localStorage` vaut `undefined`
// alors que `window.sessionStorage`, que Node ne définit pas, fonctionne — c'est
// cette asymétrie qui trahit la cause. La CI est épinglée sur Node 22 et ne voit
// donc rien, mais 260 tests tombent sur une machine à jour, et tomberaient en
// CI le jour où elle monte de version.
//
// On installe donc un stockage explicite plutôt que de dépendre de qui, de
// jsdom ou de Node, gagne la propriété. Les tests n'ont besoin que de la
// sémantique du Web Storage : des chaînes, et une remise à zéro entre deux cas.
class StockageMemoire implements Storage {
  private valeurs = new Map<string, string>();

  get length(): number {
    return this.valeurs.size;
  }

  key(index: number): string | null {
    return [...this.valeurs.keys()][index] ?? null;
  }

  getItem(cle: string): string | null {
    return this.valeurs.get(cle) ?? null;
  }

  // Le Web Storage ne stocke que des chaînes : `setItem(k, 1)` relit `'1'`.
  // Sans cette conversion, un test passerait ici et échouerait en vrai.
  setItem(cle: string, valeur: string): void {
    this.valeurs.set(cle, String(valeur));
  }

  removeItem(cle: string): void {
    this.valeurs.delete(cle);
  }

  clear(): void {
    this.valeurs.clear();
  }
}

if (!globalThis.localStorage) {
  Object.defineProperty(globalThis, 'localStorage', {
    value: new StockageMemoire(),
    configurable: true,
    writable: true,
  });
}

// Toutes les langues sont montées d'emblée dans les tests, noyaux et
// extensions de réglages : en production elles arrivent par chunks asynchrones,
// mais un test qui bascule `lang` doit voir la traduction immédiatement, sans
// await. Sans les extensions, tout rendu d'un onglet de réglages suspendrait.
import { registerDictionary, registerSettingsDict, type Lang } from '../i18n';
import { dictionaries, settingsDictionaries } from '../i18n/all';

for (const [lang, dict] of Object.entries(dictionaries)) {
  registerDictionary(lang as Lang, dict);
}
for (const [lang, dict] of Object.entries(settingsDictionaries)) {
  registerSettingsDict(lang as Lang, dict);
}
