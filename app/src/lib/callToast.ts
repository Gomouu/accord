/**
 * Traduit `event.call_ended.reason` en toast discret (voir VOICE_CALLS.md
 * §1.2). Fonction pure — décidée séparément du câblage d'événements
 * (`AppShell`) pour rester testable sans monter l'arbre React. `hangup` et
 * `superseded` ne produisent aucun toast : le premier est la fin normale d'un
 * appel (le panneau disparaît, ça suffit), le second est immédiatement suivi
 * de `event.call_accepted` (appels croisés, aucune UX distincte requise).
 */

import type { Dict } from '../i18n';
import { interpolate } from '../i18n';
import type { CallEndedReason } from './api';

export interface CallEndedToast {
  kind: 'info' | 'error';
  text: string;
}

export function callEndedToast(
  t: Dict,
  reason: CallEndedReason,
  peerName: string,
): CallEndedToast | null {
  switch (reason) {
    case 'missed':
      return { kind: 'info', text: interpolate(t.calls.missedFrom, { name: peerName }) };
    case 'busy':
      return { kind: 'info', text: interpolate(t.calls.busy, { name: peerName }) };
    case 'timeout':
      return { kind: 'info', text: t.calls.timeoutMsg };
    case 'declined':
      return { kind: 'info', text: t.calls.declinedMsg };
    case 'canceled':
      return { kind: 'info', text: t.calls.canceledMsg };
    case 'lost':
      return { kind: 'error', text: t.calls.lostMsg };
    case 'hangup':
    case 'superseded':
      return null;
  }
}
