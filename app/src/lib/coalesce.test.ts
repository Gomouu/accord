/** Tests de `coalescePerKey` : bornes du regroupement, et ce qu'il ne fait pas. */

import { describe, expect, it, vi } from 'vitest';
import { coalescePerKey } from './coalesce';

/** Promesse dont le test décide du moment de résolution. */
function suspendue(): { promesse: Promise<void>; resoudre: () => void } {
  let resoudre = (): void => {};
  const promesse = new Promise<void>((r) => {
    resoudre = r;
  });
  return { promesse, resoudre };
}

describe('coalescePerKey', () => {
  it('replie une rafale en deux exécutions au plus', async () => {
    const porte = suspendue();
    const run = vi.fn(async () => {
      await porte.promesse;
    });
    const coalesce = coalescePerKey(run);

    // Cinq appels pendant que le premier est en vol.
    const rafale = [
      coalesce('a'),
      coalesce('a'),
      coalesce('a'),
      coalesce('a'),
      coalesce('a'),
    ];
    porte.resoudre();
    await Promise.all(rafale);

    // Celui en vol, plus UN rattrapage — pas quatre.
    expect(run).toHaveBeenCalledTimes(2);
  });

  it('exécute quand même le tour de rattrapage', async () => {
    // Le point délicat : la rafale ne doit pas être simplement jetée. Ce qui
    // est arrivé pendant l'exécution doit être vu par un tour de plus, sinon
    // l'interface resterait sur un état périmé jusqu'au prochain événement.
    const porte = suspendue();
    let tours = 0;
    const coalesce = coalescePerKey(async () => {
      tours += 1;
      if (tours === 1) await porte.promesse;
    });

    const premier = coalesce('a');
    const pendant = coalesce('a');
    porte.resoudre();
    await Promise.all([premier, pendant]);

    expect(tours).toBe(2);
  });

  it('n’enchaîne pas des appels espacés', async () => {
    // Contrôle négatif : hors rafale, chaque appel s'exécute. Un compteur
    // « déjà vu » au lieu de la coalescence ferait passer le test précédent
    // et échouer celui-ci.
    const run = vi.fn(async () => {});
    const coalesce = coalescePerKey(run);

    await coalesce('a');
    await coalesce('a');
    await coalesce('a');

    expect(run).toHaveBeenCalledTimes(3);
  });

  it('garde les clés indépendantes', async () => {
    const vues: string[] = [];
    const porte = suspendue();
    const coalesce = coalescePerKey(async (k) => {
      vues.push(k);
      await porte.promesse;
    });

    const deux = [coalesce('a'), coalesce('b')];
    porte.resoudre();
    await Promise.all(deux);

    // 🔒 Une clé bavarde ne doit pas faire taire les autres.
    expect(vues).toEqual(['a', 'b']);
  });

  it('libère la clé quand le travail échoue', async () => {
    // Sans le `finally`, un rejet laisserait la clé marquée « en vol » et tout
    // événement ultérieur serait ignoré en silence — une panne durable.
    let echouer = true;
    const coalesce = coalescePerKey(async () => {
      if (echouer) throw new Error('boum');
    });

    await expect(coalesce('a')).rejects.toThrow('boum');
    echouer = false;
    await expect(coalesce('a')).resolves.toBeUndefined();
  });
});
