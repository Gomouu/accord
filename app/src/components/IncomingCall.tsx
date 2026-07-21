/**
 * Overlay plein écran d'appel entrant (phase `incoming_ringing`), monté une
 * fois dans `AppShell` (visible quelle que soit la vue courante). Avatar +
 * nom du pair, Accepter (vert) / Refuser (rouge). Joue la sonnerie en boucle
 * (`lib/ringtone`) tant que la phase reste `incoming_ringing` — le Ne pas
 * déranger et la préférence de son coupent uniquement le son, jamais
 * l'overlay lui-même (voir `lib/ringtone.ts`).
 */

import { useEffect, useRef } from 'react';
import { bouclerTab } from '../lib/focus';
import { startRingtone, stopRingtone } from '../lib/ringtone';
import { useCalls } from '../stores/calls';
import {
  avatarDecorationOf,
  avatarOf,
  displayNameOf,
  useFriends,
} from '../stores/friends';
import { useUi, useT } from '../stores/ui';
import { Avatar } from './Avatar';
import { PhoneIcon, PhoneOffIcon } from './ContextMenu';

const ROUND_BUTTON =
  'flex h-14 w-14 shrink-0 items-center justify-center rounded-full transition-transform duration-fast active:scale-95 hover:brightness-110 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2 focus-visible:ring-offset-modal';

export function IncomingCall() {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const phase = useCalls((s) => s.phase);
  const peer = useCalls((s) => s.peer);
  const accept = useCalls((s) => s.accept);
  const decline = useCalls((s) => s.decline);
  const contacts = useFriends((s) => s.contacts);

  const ringing = phase === 'incoming_ringing' && peer !== null;
  const overlayRef = useRef<HTMLDivElement>(null);
  const declineRef = useRef<HTMLButtonElement>(null);

  // La sonnerie suit strictement la phase : elle démarre à l'entrée en
  // sonnerie entrante et s'arrête immédiatement à toute autre transition
  // (acceptée, refusée, annulée par l'appelant…), sans dépendre du démontage.
  useEffect(() => {
    if (!ringing) {
      stopRingtone();
      return undefined;
    }
    startRingtone();
    return () => stopRingtone();
  }, [ringing]);

  // Overlay modal : le focus entre sur « Refuser » (l'action sans risque —
  // Entrée n'ouvre pas le micro par accident) et revient à l'élément
  // précédent à la fermeture, quelle que soit l'issue de la sonnerie.
  useEffect(() => {
    if (!ringing) return undefined;
    const declencheur =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    declineRef.current?.focus();
    return () => {
      if (declencheur !== null && declencheur.isConnected) declencheur.focus();
    };
  }, [ringing]);

  if (!ringing || peer === null) return null;

  const name = displayNameOf(contacts, peer);
  const avatarHash = avatarOf(contacts, peer);
  const onActionError = (): void => toast('error', t.errors.actionFailed);

  return (
    <div
      ref={overlayRef}
      role="dialog"
      aria-modal="true"
      aria-label={`${name} ${t.calls.incomingRinging}`}
      onKeyDown={(e) => {
        // Échap équivaut au bouton « Refuser » : seule fermeture possible.
        if (e.key === 'Escape') {
          e.preventDefault();
          decline().catch(onActionError);
          return;
        }
        bouclerTab(e, overlayRef.current);
      }}
      className="liquid-overlay fixed inset-0 z-[60] flex items-center justify-center bg-black/50"
    >
      <div className="glass-strong flex w-80 max-w-[90vw] flex-col items-center gap-5 rounded-xl p-6 shadow-3">
        <Avatar
          id={peer}
          name={name}
          size={72}
          avatarHash={avatarHash}
          hint={peer}
          decoration={avatarDecorationOf(contacts, peer)}
        />
        <div className="text-center">
          <div className="text-lg font-semibold text-header">{name}</div>
          <div className="mt-0.5 text-sm text-muted">{t.calls.incomingRinging}</div>
        </div>
        <div className="flex items-center gap-8">
          <button
            ref={declineRef}
            type="button"
            aria-label={t.calls.decline}
            title={t.calls.decline}
            onClick={() => {
              decline().catch(onActionError);
            }}
            className={`${ROUND_BUTTON} bg-red text-on-red focus-visible:ring-red`}
          >
            <PhoneOffIcon size={26} />
          </button>
          <button
            type="button"
            aria-label={t.calls.accept}
            title={t.calls.accept}
            onClick={() => {
              accept().catch(onActionError);
            }}
            className={`${ROUND_BUTTON} bg-green text-on-green focus-visible:ring-green`}
          >
            <PhoneIcon size={26} />
          </button>
        </div>
      </div>
    </div>
  );
}
