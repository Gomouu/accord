/**
 * Carte de sondage (MsgBody kind 7, `groups.polls.*`, D-048) : question, une
 * barre par option (vote au clic, pourcentage + décompte, mise en évidence
 * du vote courant), total de votes et bouton de fermeture (auteur ou
 * `MANAGE_CHANNELS`, confirmation en place via `ConfirmButton`).
 *
 * Le dépouillement vient de `groups.state.polls` — `poll` peut manquer
 * (sondage tout juste envoyé, état pas encore rechargé) : la carte reste
 * alors votable à zéro (voir `pollResults`). `resultsAvailable = false`
 * (nœud plus ancien sans champ `polls`, ou hors contexte de groupe) désactive
 * le vote et affiche un repli neutre plutôt que de suggérer un état qui ne se
 * résoudra jamais.
 */

import { interpolate } from '../i18n';
import type { GroupPoll } from '../lib/api';
import { pollResults } from '../stores/groups';
import { useT } from '../stores/ui';
import { ConfirmButton } from './server/controls';

export interface PollCardProps {
  question: string;
  options: string[];
  poll: GroupPoll | undefined;
  /** Faux si `groups.state.polls` est absent (nœud plus ancien, ou hors contexte de groupe). */
  resultsAvailable: boolean;
  /** Auteur du sondage ou porteur de `MANAGE_CHANNELS`, dépouillement disponible. */
  canClose: boolean;
  onVote: (optionIndex: number) => void;
  onClose: () => void;
}

export function PollCard({
  question,
  options,
  poll,
  resultsAvailable,
  canClose,
  onVote,
  onClose,
}: PollCardProps) {
  const t = useT();
  const results = pollResults(poll, options.length);
  const closed = poll?.closed ?? false;
  const votingDisabled = !resultsAvailable || closed;

  return (
    <div className="max-w-md rounded-lg bg-input p-3">
      <p className="font-medium text-header">{question}</p>
      <div className="mt-2.5 space-y-1.5">
        {options.map((option, index) => {
          const pct = results.percentages[index] ?? 0;
          const count = results.counts[index] ?? 0;
          const mine = results.myVote === index;
          return (
            <button
              key={index}
              type="button"
              disabled={votingDisabled}
              aria-pressed={mine}
              aria-label={interpolate(t.groups.pollVoteFor, { option })}
              onClick={() => onVote(index)}
              className={`relative block w-full overflow-hidden rounded-md border px-3 py-2 text-start text-sm transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-input disabled:cursor-default ${
                mine
                  ? 'border-blurple bg-blurple/10'
                  : 'border-transparent bg-chat-hover/70 enabled:hover:border-blurple/40'
              }`}
            >
              <span
                aria-hidden
                className="absolute inset-y-0 start-0 w-full origin-left bg-blurple/10 transition-transform duration-normal ease-expo"
                style={{ transform: `scaleX(${pct / 100})` }}
              />
              <span className="relative flex items-center justify-between gap-3">
                <span className="min-w-0 truncate text-norm">{option}</span>
                <span className="shrink-0 text-xs tabular-nums text-faint">
                  {pct}% · {count}
                </span>
              </span>
            </button>
          );
        })}
      </div>
      {resultsAvailable ? (
        <div className="mt-2.5 flex items-center justify-between gap-2 text-xs text-faint">
          <span>
            {interpolate(t.groups.pollVotesCount, { count: String(results.total) })}
          </span>
          {closed ? (
            <span className="font-medium text-norm">{t.groups.pollClosed}</span>
          ) : (
            canClose && (
              <ConfirmButton
                action={t.groups.pollClose}
                question={t.groups.pollCloseConfirm}
                onConfirm={onClose}
              />
            )
          )}
        </div>
      ) : (
        <p className="mt-2.5 text-xs italic text-faint">
          {t.groups.pollResultsUnavailable}
        </p>
      )}
    </div>
  );
}
