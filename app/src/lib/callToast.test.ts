/**
 * Tests de `callEndedToast` : traduction pure de `event.call_ended.reason` en
 * toast (voir VOICE_CALLS.md §1.2) — `hangup`/`superseded` ne produisent
 * aucun toast, `missed`/`busy` interpolent le nom du pair.
 */

import { describe, expect, it } from 'vitest';
import { fr } from '../i18n/fr';
import { callEndedToast } from './callToast';

describe('callEndedToast', () => {
  it('missed : toast info interpolé avec le nom du pair', () => {
    expect(callEndedToast(fr, 'missed', 'Alice')).toEqual({
      kind: 'info',
      text: 'Appel manqué de Alice',
    });
  });

  it('busy : toast info interpolé avec le nom du pair', () => {
    expect(callEndedToast(fr, 'busy', 'Alice')).toEqual({
      kind: 'info',
      text: 'Alice est déjà en appel',
    });
  });

  it('timeout : toast discret sans nom', () => {
    expect(callEndedToast(fr, 'timeout', 'Alice')).toEqual({
      kind: 'info',
      text: 'Aucune réponse',
    });
  });

  it('declined : toast discret sans nom', () => {
    expect(callEndedToast(fr, 'declined', 'Alice')).toEqual({
      kind: 'info',
      text: 'Appel refusé',
    });
  });

  it('canceled : toast discret', () => {
    expect(callEndedToast(fr, 'canceled', 'Alice')).toEqual({
      kind: 'info',
      text: 'Appel annulé',
    });
  });

  it('lost : toast d’erreur (perte réseau anormale)', () => {
    expect(callEndedToast(fr, 'lost', 'Alice')).toEqual({
      kind: 'error',
      text: 'Appel interrompu (connexion perdue)',
    });
  });

  it('hangup : aucun toast (fin normale d’appel)', () => {
    expect(callEndedToast(fr, 'hangup', 'Alice')).toBeNull();
  });

  it('superseded : aucun toast (call_accepted suit immédiatement)', () => {
    expect(callEndedToast(fr, 'superseded', 'Alice')).toBeNull();
  });
});
