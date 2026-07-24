/**
 * Préparation de l'environnement de test : étend `expect` de Vitest avec les
 * matchers DOM de Testing Library (`toBeInTheDocument`, etc.).
 */

import '@testing-library/jest-dom/vitest';

// Toutes les langues sont montées d'emblée dans les tests : en production
// elles arrivent par chunks asynchrones, mais un test qui bascule `lang` doit
// voir la traduction immédiatement, sans await.
import { registerDictionary, type Lang } from '../i18n';
import { dictionaries } from '../i18n/all';

for (const [lang, dict] of Object.entries(dictionaries)) {
  registerDictionary(lang as Lang, dict);
}
