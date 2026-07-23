/**
 * Schedule-message dialog (F1 hook): compose a message and pick a send time.
 * Purely local — `dm.schedule` stores it; the node's maintenance loop sends it
 * through the normal path when due. Same modal conventions as the rest of the
 * repo: role="dialog", Escape closes, Tab looped, focus returned.
 */

import { useEffect, useRef, useState } from 'react';
import { api } from '../lib/client';
import { bouclerTab } from '../lib/focus';
import { usePlanning } from '../stores/planning';
import { useT, useUi } from '../stores/ui';
import { CloseIcon } from './ContextMenu';

/** `<input type="datetime-local">` value one hour from now (local time). */
function defaultWhen(): string {
  const d = new Date(Date.now() + 60 * 60 * 1000);
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

export function ScheduleMessageDialog({
  peer,
  onClose,
}: {
  peer: string;
  onClose: () => void;
}) {
  const t = useT();
  const toast = useUi((s) => s.toast);
  const reloadScheduled = usePlanning((s) => s.loadScheduled);
  const ref = useRef<HTMLDivElement>(null);
  const [body, setBody] = useState('');
  const [when, setWhen] = useState(defaultWhen);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    const previous =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
      else if (e.key === 'Tab') bouclerTab(e, ref.current);
    };
    window.addEventListener('keydown', onKey);
    ref.current?.focus();
    return () => {
      window.removeEventListener('keydown', onKey);
      if (previous !== null && previous.isConnected) previous.focus();
    };
  }, [onClose]);

  const canSubmit = body.trim() !== '' && !Number.isNaN(new Date(when).getTime());

  const submit = (): void => {
    const ms = new Date(when).getTime();
    if (!canSubmit || busy) return;
    setBusy(true);
    api
      .dmSchedule(peer, body, ms)
      .then(() => {
        void reloadScheduled().catch(() => {
          // Best effort: the list reloads when the panel opens.
        });
        onClose();
      })
      .catch(() => toast('error', t.errors.actionFailed))
      .finally(() => setBusy(false));
  };

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={t.planning.scheduleDialogTitle}
        tabIndex={-1}
        className="glass modal-panel-enter flex w-[26rem] max-w-[92vw] flex-col overflow-hidden rounded-xl shadow-3 focus:outline-none"
      >
        <div className="flex items-center justify-between px-5 pt-5">
          <h2 className="text-lg font-semibold text-header">
            {t.planning.scheduleDialogTitle}
          </h2>
          <button
            type="button"
            aria-label={t.app.close}
            onClick={onClose}
            className="rounded-sm p-1 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
          >
            <CloseIcon size={20} />
          </button>
        </div>
        <div className="flex flex-col gap-3 px-5 pb-5 pt-3">
          <textarea
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder={t.planning.scheduleBodyPlaceholder}
            rows={3}
            className="resize-none rounded-lg bg-input px-3 py-2 text-sm text-norm placeholder:text-faint focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
          />
          <label className="flex flex-col gap-1 text-sm text-muted">
            {t.planning.scheduleWhen}
            <input
              type="datetime-local"
              value={when}
              onChange={(e) => setWhen(e.target.value)}
              className="rounded-lg bg-input px-3 py-2 text-sm text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple"
            />
          </label>
          <button
            type="button"
            disabled={busy || !canSubmit}
            onClick={submit}
            className="mt-1 rounded-full bg-blurple px-3 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-[0.98] disabled:opacity-50"
          >
            {t.planning.scheduleSubmit}
          </button>
        </div>
      </div>
    </div>
  );
}
