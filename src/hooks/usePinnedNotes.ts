import { useLists } from './useLists';

const PINNED_NOTES_KIND = 10001;

/**
 * Hook to manage pinned notes (Kind 10001)
 * Events the user intends to showcase on their profile page
 */
export function usePinnedNotes() {
  const list = useLists(PINNED_NOTES_KIND);

  // Check if a note is pinned
  const isPinned = (eventId: string): boolean => {
    return list.hasItem('e', eventId);
  };

  // Pin a note
  const pinNote = async (eventId: string, relay?: string) => {
    return await list.addItem.mutateAsync({ type: 'e', value: eventId, relay });
  };

  // Unpin a note
  const unpinNote = async (eventId: string) => {
    return await list.removeItem.mutateAsync({ type: 'e', value: eventId });
  };

  // Toggle pin for a note
  const togglePin = async (eventId: string, relay?: string) => {
    return await list.toggleItem.mutateAsync({ type: 'e', value: eventId, relay });
  };

  return {
    ...list,
    isPinned,
    pinNote,
    unpinNote,
    togglePin,
  };
}
