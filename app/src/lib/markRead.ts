import { useDms } from '../stores/dms';
import { useFriends } from '../stores/friends';
import { marquerServeurLu } from './markServerRead';

export async function markDmRead(peer: string): Promise<boolean> {
  const latestLamport = (): number | undefined =>
    useDms.getState().conversations[peer]?.at(-1)?.lamport;
  let lamport = latestLamport();
  if (lamport === undefined) {
    await useDms.getState().refresh(peer);
    lamport = latestLamport();
  }
  if (lamport === undefined) return false;
  await useFriends.getState().markRead(peer, lamport);
  return true;
}

export async function markAllRead(
  groupIds: readonly string[],
  dmPeers: readonly string[],
): Promise<void> {
  await Promise.all([
    ...groupIds.map((groupId) => marquerServeurLu(groupId)),
    ...dmPeers.map((peer) => markDmRead(peer)),
  ]);
}
