/**
 * Ligne discrète « X est en train d'écrire… » sous la zone de saisie,
 * alimentée par le store typing (événements éphémères, expiration ~4 s).
 * Rendue dans le remplissage bas de la zone de saisie via une marge
 * négative : son apparition ne décale pas la mise en page.
 */

import { interpolate } from '../i18n';
import { useFriends, displayNameOf } from '../stores/friends';
import { useTyping } from '../stores/typing';
import { useT } from '../stores/ui';

/** Au-delà de ce nombre d'écrivains, les noms ne sont plus détaillés. */
const MAX_NAMED_WRITERS = 2;

export function TypingIndicator({ typingKey }: { typingKey: string }) {
  const t = useT();
  const contacts = useFriends((s) => s.contacts);
  const writers = useTyping((s) => s.writers[typingKey]);
  const pubkeys = Object.keys(writers ?? {});
  if (pubkeys.length === 0) return null;

  const names = pubkeys.map((pubkey) => displayNameOf(contacts, pubkey));
  const label =
    names.length === 1
      ? interpolate(t.dm.typingOne, { name: names[0] ?? '' })
      : names.length === MAX_NAMED_WRITERS
        ? interpolate(t.dm.typingTwo, { a: names[0] ?? '', b: names[1] ?? '' })
        : t.dm.typingMany;

  return (
    <div
      role="status"
      className="view-enter -mt-5 flex h-5 items-center gap-1.5 px-6 text-xs text-muted"
    >
      <span aria-hidden className="animate-pulse font-bold tracking-widest">
        …
      </span>
      <span className="truncate">{label}</span>
    </div>
  );
}
