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

export function TypingIndicator({
  typingKey,
  nameOf,
}: {
  typingKey: string;
  nameOf?: ((pubkey: string) => string) | undefined;
}) {
  const t = useT();
  const contacts = useFriends((s) => s.contacts);
  const writers = useTyping((s) => s.writers[typingKey]);
  const pubkeys = Object.keys(writers ?? {});

  const names = pubkeys.map(
    (pubkey) => nameOf?.(pubkey) ?? displayNameOf(contacts, pubkey),
  );
  const label =
    names.length === 0
      ? null
      : names.length === 1
        ? interpolate(t.dm.typingOne, { name: names[0] ?? '' })
        : names.length === MAX_NAMED_WRITERS
          ? interpolate(t.dm.typingTwo, { a: names[0] ?? '', b: names[1] ?? '' })
          : t.dm.typingMany;

  return (
    <div className="flex h-5 shrink-0 items-center px-6">
      {label !== null && (
        <div
          role="status"
          aria-live="polite"
          className="view-enter flex min-w-0 items-center gap-2 text-xs text-muted"
        >
          <span aria-hidden className="flex shrink-0 items-center gap-0.5">
            {[0, 1, 2].map((index) => (
              <span
                key={index}
                className="typing-dot h-1 w-1 rounded-full bg-muted"
                style={{ animationDelay: `${index * 120}ms` }}
              />
            ))}
          </span>
          <span className="truncate">{label}</span>
        </div>
      )}
    </div>
  );
}
