/**
 * Miroir des préférences de compte, côté interface.
 *
 * Ce module tenait la comparaison « dernier écrit gagne » de ce côté-ci sans
 * aucun test : l'agent qui l'a écrit a été interrompu avant d'en poser. Ce
 * fichier couvre ce qui casse silencieusement — l'ordre, l'idempotence, la
 * liste blanche, et surtout la boucle que la suspension du miroir évite.
 */

import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { Mock } from 'vitest';

vi.mock('./client', () => ({
  api: {
    setPref: vi.fn(() => Promise.resolve(0)),
    listPrefs: vi.fn(() => Promise.resolve([])),
  },
}));

import { api } from './client';
import {
  applyRemotePref,
  hydrateSyncedPrefs,
  mirrorSyncedPref,
  prefSyncedAt,
  recordPrefSyncedAt,
  registerPrefApplier,
} from './prefSync';

const setPrefMock = api.setPref as unknown as Mock;
const listPrefsMock = api.listPrefs as unknown as Mock;

/** Une clé bien de la liste blanche, et une valeur qui ne veut rien dire. */
const CLE = 'accord.theme';

let applique: [string, string][] = [];

beforeEach(() => {
  window.localStorage.clear();
  applique = [];
  setPrefMock.mockClear();
  setPrefMock.mockResolvedValue(0);
  listPrefsMock.mockClear();
  listPrefsMock.mockResolvedValue([]);
  registerPrefApplier((key, value) => {
    applique.push([key, value]);
  });
});

describe('applyRemotePref', () => {
  it('adopte une valeur plus récente que ce que cette machine a poussé', () => {
    // Arrange
    recordPrefSyncedAt(CLE, 1_000);

    // Act
    const adoptee = applyRemotePref(CLE, 'midnight', 2_000);

    // Assert
    expect(adoptee).toBe(true);
    expect(applique).toEqual([[CLE, 'midnight']]);
    expect(prefSyncedAt(CLE)).toBe(2_000);
  });

  it('refuse une valeur plus ancienne, et le même événement reçu deux fois', () => {
    // Arrange
    recordPrefSyncedAt(CLE, 2_000);

    // Act / Assert — plus ancienne.
    expect(applyRemotePref(CLE, 'ancienne', 1_000)).toBe(false);
    // …et à horodatage ÉGAL : il n'y a rien à départager, garder l'existant
    // rend l'opération idempotente.
    expect(applyRemotePref(CLE, 'egale', 2_000)).toBe(false);
    expect(applique).toEqual([]);
  });

  it('ignore une clé hors de la liste blanche', () => {
    // `autoLockMinutes` est délibérément un réglage de MACHINE : une machine
    // partagée veut un verrouillage plus agressif qu'un fixe à la maison.
    expect(applyRemotePref('accord.autoLockMinutes', '5', 9_000)).toBe(false);
    expect(applique).toEqual([]);
  });

  it('🔒 n’écho pas la valeur adoptée vers le nœud', async () => {
    // Le cœur de l'affaire. Appliquer une préférence passe par le store, donc
    // par le miroir : sans la suspension, adopter la valeur d'un appareil la
    // lui renverrait, il l'adopterait à son tour, et les deux machines se la
    // rejoueraient sans fin. On vérifie ici que l'applicateur peut écrire sans
    // que rien ne reparte.
    registerPrefApplier((key, value) => {
      applique.push([key, value]);
      // Ce que fait réellement le store en écrivant la préférence.
      mirrorSyncedPref(key, value);
    });

    applyRemotePref(CLE, 'midnight', 5_000);
    // ⚠️ Attente indispensable, et ce test l'a appris à ses dépens : le miroir
    // n'appelle pas `setPref` tout de suite, il l'enveloppe dans une
    // micro-tâche. Une assertion synchrone passait donc même en retirant la
    // suspension — elle constatait seulement que rien n'avait ENCORE eu lieu.
    await new Promise((resoudre) => setTimeout(resoudre, 0));

    expect(applique).toEqual([[CLE, 'midnight']]);
    expect(setPrefMock).not.toHaveBeenCalled();
  });
});

describe('mirrorSyncedPref', () => {
  it('pousse une clé de la liste blanche et retient l’horodatage rendu', async () => {
    setPrefMock.mockResolvedValue(7_000);

    mirrorSyncedPref(CLE, 'midnight');
    await vi.waitFor(() => expect(prefSyncedAt(CLE)).toBe(7_000));

    expect(setPrefMock).toHaveBeenCalledWith(CLE, 'midnight');
  });

  it('ne pousse rien pour une clé hors liste blanche', () => {
    mirrorSyncedPref('accord.fontScale', '1.2');
    expect(setPrefMock).not.toHaveBeenCalled();
  });

  it('reste silencieux quand le nœud est injoignable', async () => {
    // Le réglage a déjà pris localement ; un miroir qui échoue ne doit ni
    // lever, ni faire avancer l'horodatage.
    setPrefMock.mockRejectedValue(new Error('hors ligne'));

    expect(() => {
      mirrorSyncedPref(CLE, 'midnight');
    }).not.toThrow();
    await vi.waitFor(() => expect(setPrefMock).toHaveBeenCalled());
    expect(prefSyncedAt(CLE)).toBe(0);
  });
});

describe('hydrateSyncedPrefs', () => {
  it('adopte au démarrage ce que le compte porte de plus récent', async () => {
    listPrefsMock.mockResolvedValue([
      { key: CLE, value: 'midnight', at_ms: 3_000 },
      { key: 'accord.lang', value: 'es', at_ms: 3_000 },
    ]);

    await hydrateSyncedPrefs();

    expect(applique).toEqual([
      [CLE, 'midnight'],
      ['accord.lang', 'es'],
    ]);
  });

  it('laisse la machine sur ses valeurs quand le nœud ne répond pas', async () => {
    listPrefsMock.mockRejectedValue(new Error('nœud injoignable'));

    await expect(hydrateSyncedPrefs()).resolves.toBeUndefined();
    expect(applique).toEqual([]);
  });
});
