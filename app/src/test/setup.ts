/**
 * Préparation de l'environnement de test : étend `expect` de Vitest avec les
 * matchers DOM de Testing Library (`toBeInTheDocument`, etc.).
 */

import '@testing-library/jest-dom/vitest';

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
