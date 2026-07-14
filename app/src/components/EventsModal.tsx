/**
 * Panneau des événements planifiés d'un serveur (`groups.events.*`) : liste
 * (titre, date/heure, auteur, salon vocal optionnel, compte de RSVP),
 * bascule « Ça m'intéresse » par membre, et formulaire de création/édition
 * réservé à `MANAGE_CHANNELS` (ou à l'auteur pour éditer/supprimer son propre
 * événement). Même coquille flottante que les autres modales
 * (`bg-black/75 backdrop-blur-sm`, panneau `.glass`), ouvert via
 * `ui.modal = { kind: 'events', groupId }`.
 */

import { useEffect, useRef, useState } from 'react';
import { interpolate } from '../i18n';
import type { GroupEvent, GroupStateJson } from '../lib/api';
import { formatEventDateTime } from '../lib/format';
import { displayNameOf, useFriends } from '../stores/friends';
import {
  nicknameOf,
  sortEvents,
  useGroups,
  hasPerm,
  PERMISSIONS,
} from '../stores/groups';
import { selfDisplayName, useSession } from '../stores/session';
import { useUi, useT } from '../stores/ui';
import { CloseIcon } from './ContextMenu';
import { ConfirmButton, messageOf } from './server/controls';

/** Bornes du titre d'un événement (contrat `groups.events.create/edit`). */
const EVENT_TITLE_MIN = 2;
const EVENT_TITLE_MAX = 100;
/** Borne de la description d'un événement (contrat). */
const EVENT_DESCRIPTION_MAX = 1024;
/** Plafond d'événements par serveur (indication client, la borne fait foi côté nœud). */
const EVENT_MAX_PAR_SERVEUR = 25;

/** Valeur `datetime-local` (heure murale locale) d'une échéance en ms. */
function toDatetimeLocalValue(ms: number): string {
  const d = new Date(ms);
  const pad = (n: number): string => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/** Formulaire de création/édition d'un événement (`existing === null` : création). */
function EventForm({
  groupId,
  state,
  existing,
  onCancel,
  onDone,
}: {
  groupId: string;
  state: GroupStateJson;
  existing: GroupEvent | null;
  onCancel: () => void;
  onDone: () => void;
}) {
  const t = useT();
  const createEvent = useGroups((s) => s.createEvent);
  const editEvent = useGroups((s) => s.editEvent);
  const [title, setTitle] = useState(existing?.title ?? '');
  const [description, setDescription] = useState(existing?.description ?? '');
  const [datetime, setDatetime] = useState(
    existing !== null ? toDatetimeLocalValue(existing.start_ms) : '',
  );
  const [channelId, setChannelId] = useState<string>(existing?.channel_id ?? '');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const voiceChannels = state.channels.filter((c) => c.kind === 'voice');
  const trimmedTitle = title.trim();
  const trimmedDescription = description.trim();
  const startMs = datetime === '' ? null : new Date(datetime).getTime();
  const validTitle =
    trimmedTitle.length >= EVENT_TITLE_MIN && trimmedTitle.length <= EVENT_TITLE_MAX;
  const validDescription = trimmedDescription.length <= EVENT_DESCRIPTION_MAX;
  const validDate = startMs !== null && Number.isFinite(startMs);
  const canSubmit = validTitle && validDescription && validDate && !busy;

  const submit = async (): Promise<void> => {
    if (!canSubmit || startMs === null) return;
    setBusy(true);
    setError(null);
    try {
      const fields = {
        title: trimmedTitle,
        description: trimmedDescription,
        startMs,
        channelId: channelId === '' ? null : channelId,
      };
      if (existing === null) await createEvent(groupId, fields);
      else await editEvent(groupId, existing.event_id, fields);
      onDone();
    } catch (e) {
      setError(messageOf(e, t.errors.actionFailed));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <h3 className="mb-3 text-sm font-semibold text-header">
        {existing === null ? t.groups.eventCreateTitle : t.groups.eventEditTitle}
      </h3>
      <label
        htmlFor="event-title"
        className="mb-1 block text-xs font-medium uppercase tracking-wide text-faint"
      >
        {t.groups.eventTitleLabel}
      </label>
      <input
        id="event-title"
        value={title}
        maxLength={EVENT_TITLE_MAX + 8}
        onChange={(e) => setTitle(e.target.value)}
        className="mb-3 w-full rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
      />
      <label
        htmlFor="event-description"
        className="mb-1 block text-xs font-medium uppercase tracking-wide text-faint"
      >
        {t.groups.eventDescriptionLabel}
      </label>
      <textarea
        id="event-description"
        value={description}
        rows={3}
        maxLength={EVENT_DESCRIPTION_MAX}
        onChange={(e) => setDescription(e.target.value)}
        className="mb-3 w-full resize-none rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm placeholder-faint outline-none transition-colors duration-fast focus:border-blurple/50"
      />
      <label
        htmlFor="event-datetime"
        className="mb-1 block text-xs font-medium uppercase tracking-wide text-faint"
      >
        {t.groups.eventDateLabel}
      </label>
      <input
        id="event-datetime"
        type="datetime-local"
        value={datetime}
        onChange={(e) => setDatetime(e.target.value)}
        className="mb-3 w-full rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm outline-none transition-colors duration-fast focus:border-blurple/50"
      />
      <label
        htmlFor="event-channel"
        className="mb-1 block text-xs font-medium uppercase tracking-wide text-faint"
      >
        {t.groups.eventChannelLabel}
      </label>
      <select
        id="event-channel"
        value={channelId}
        onChange={(e) => setChannelId(e.target.value)}
        className="mb-4 w-full rounded-md border border-transparent bg-input px-3 py-2 text-sm text-norm outline-none transition-colors duration-fast focus:border-blurple/50"
      >
        <option value="">{t.groups.eventChannelNone}</option>
        {voiceChannels.map((c) => (
          <option key={c.channel_id} value={c.channel_id}>
            {c.name}
          </option>
        ))}
      </select>
      {error !== null && (
        <p className="mb-3 text-sm text-red" role="alert">
          {error}
        </p>
      )}
      <div className="flex justify-end gap-3">
        <button
          type="button"
          onClick={onCancel}
          className="rounded-sm px-4 py-2 text-sm font-medium text-muted transition-colors duration-fast hover:bg-chat-hover hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal"
        >
          {t.app.cancel}
        </button>
        <button
          type="button"
          disabled={!canSubmit}
          onClick={() => void submit()}
          className="rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-50"
        >
          {existing === null ? t.groups.eventCreateAction : t.groups.eventSave}
        </button>
      </div>
    </div>
  );
}

export function EventsModal({ groupId }: { groupId: string }) {
  const t = useT();
  const lang = useUi((s) => s.lang);
  const timeFormat = useUi((s) => s.timeFormat);
  const closeModal = useUi((s) => s.closeModal);
  const toast = useUi((s) => s.toast);
  const contacts = useFriends((s) => s.contacts);
  const self = useSession((s) => s.self);
  const state = useGroups((s) => s.states[groupId]);
  const rsvpEvent = useGroups((s) => s.rsvpEvent);
  const deleteEvent = useGroups((s) => s.deleteEvent);
  const [formTarget, setFormTarget] = useState<GroupEvent | 'new' | null>(null);
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.key !== 'Escape') return;
      if (formTarget !== null) setFormTarget(null);
      else closeModal();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [closeModal, formTarget]);

  if (state === undefined) return null;

  const canManage = hasPerm(state.my_permissions, PERMISSIONS.MANAGE_CHANNELS);
  const events = sortEvents(state.events ?? []);
  const atCap = events.length >= EVENT_MAX_PAR_SERVEUR;

  const nameOf = (pubkey: string): string => {
    const nick = nicknameOf(state, pubkey);
    if (self !== null && pubkey === self.pubkey) return nick ?? selfDisplayName(self);
    return nick ?? displayNameOf(contacts, pubkey);
  };
  const voiceChannelName = (channelId: string | null): string | null =>
    channelId === null
      ? null
      : (state.channels.find((c) => c.channel_id === channelId)?.name ?? null);

  const onToggleRsvp = (event: GroupEvent): void => {
    rsvpEvent(groupId, event.event_id, !event.rsvped).catch(() =>
      toast('error', t.errors.actionFailed),
    );
  };
  const onDelete = (event: GroupEvent): void => {
    deleteEvent(groupId, event.event_id).catch((e: unknown) =>
      toast('error', messageOf(e, t.errors.actionFailed)),
    );
  };

  return (
    <div
      className="modal-overlay-enter fixed inset-0 z-40 flex items-center justify-center bg-black/75 backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) closeModal();
      }}
    >
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={t.groups.eventsTitle}
        className="glass modal-panel-enter flex max-h-[85vh] w-[560px] max-w-[92vw] flex-col overflow-hidden rounded-xl shadow-3"
      >
        <div className="flex items-center justify-between border-b border-input/50 p-5 pb-4">
          <h2 className="text-lg font-semibold text-header">{t.groups.eventsTitle}</h2>
          <button
            type="button"
            aria-label={t.app.close}
            onClick={closeModal}
            className="rounded-sm p-1 text-faint transition-colors duration-fast hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal active:scale-95"
          >
            <CloseIcon size={20} />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto p-5 pt-4">
          {formTarget !== null ? (
            <EventForm
              groupId={groupId}
              state={state}
              existing={formTarget === 'new' ? null : formTarget}
              onCancel={() => setFormTarget(null)}
              onDone={() => setFormTarget(null)}
            />
          ) : (
            <>
              {canManage && (
                <>
                  <button
                    type="button"
                    disabled={atCap}
                    onClick={() => setFormTarget('new')}
                    className="mb-1 w-full rounded-lg bg-blurple px-4 py-2 text-sm font-medium text-white transition-colors duration-fast hover:bg-blurple-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-modal disabled:opacity-50"
                  >
                    {t.groups.eventCreate}
                  </button>
                  {atCap && (
                    <p className="mb-3 text-xs text-faint">{t.groups.eventLimit}</p>
                  )}
                </>
              )}
              {events.length === 0 ? (
                <p className="py-6 text-center text-sm text-muted">
                  {t.groups.eventsEmpty}
                </p>
              ) : (
                <div className="mt-3 space-y-2">
                  {events.map((event) => {
                    const channelName = voiceChannelName(event.channel_id);
                    const isAuthor = self !== null && event.author === self.pubkey;
                    const canEditEvent = canManage || isAuthor;
                    return (
                      <div key={event.event_id} className="rounded-lg bg-sidebar p-3">
                        <div className="flex items-start justify-between gap-2">
                          <div className="min-w-0">
                            <div className="truncate font-semibold text-header">
                              {event.title}
                            </div>
                            <div className="text-xs text-muted">
                              {formatEventDateTime(event.start_ms, lang, timeFormat)}
                            </div>
                          </div>
                          <button
                            type="button"
                            aria-pressed={event.rsvped}
                            onClick={() => onToggleRsvp(event)}
                            className={`shrink-0 rounded-full px-3 py-1 text-xs font-medium transition-colors duration-fast focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar ${
                              event.rsvped
                                ? 'bg-green text-on-green hover:brightness-110'
                                : 'border border-input text-muted hover:text-norm'
                            }`}
                          >
                            {event.rsvped
                              ? t.groups.eventInterested
                              : t.groups.eventInterestedAction}
                          </button>
                        </div>
                        {event.description !== '' && (
                          <p className="mt-1.5 whitespace-pre-wrap break-words text-sm text-norm">
                            {event.description}
                          </p>
                        )}
                        <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-faint">
                          <span>
                            {interpolate(t.groups.eventBy, {
                              name: nameOf(event.author),
                            })}
                          </span>
                          {channelName !== null && (
                            <span>
                              {interpolate(t.groups.eventVoiceChannel, {
                                name: channelName,
                              })}
                            </span>
                          )}
                          <span>
                            {interpolate(t.groups.eventRsvpCount, {
                              count: String(event.rsvp_count),
                            })}
                          </span>
                        </div>
                        {canEditEvent && (
                          <div className="mt-2 flex items-center gap-2">
                            <button
                              type="button"
                              onClick={() => setFormTarget(event)}
                              className="rounded-md px-2 py-1 text-xs font-medium text-muted transition-colors hover:text-norm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blurple focus-visible:ring-offset-2 focus-visible:ring-offset-sidebar"
                            >
                              {t.groups.eventEdit}
                            </button>
                            <ConfirmButton
                              action={t.groups.eventDelete}
                              question={interpolate(t.groups.eventDeleteConfirm, {
                                title: event.title,
                              })}
                              onConfirm={() => onDelete(event)}
                            />
                          </div>
                        )}
                      </div>
                    );
                  })}
                </div>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}
