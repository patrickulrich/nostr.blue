import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useCurrentUser } from './useCurrentUser';

const BOOKMARKS_KIND = 10003;

export interface Bookmark {
  type: 'note' | 'article' | 'hashtag' | 'url';
  value: string;
  relay?: string;
}

/**
 * Hook to manage user bookmarks (NIP-51 kind 10003).
 * Provides functionality to fetch, add, remove, and toggle bookmarks.
 *
 * @returns Object containing bookmarks, loading state, and mutation functions
 */
export function useBookmarks() {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const queryClient = useQueryClient();
  const userPubkey = user?.pubkey;

  // Fetch the user's bookmarks event (Kind 10003)
  const { data: bookmarksEvent, isLoading } = useQuery<NostrEvent | null>({
    queryKey: ['bookmarks', userPubkey],
    queryFn: async ({ signal }) => {
      if (!userPubkey) return null;

      try {
        const events = await nostr.query(
          [{ kinds: [BOOKMARKS_KIND], authors: [userPubkey], limit: 1 }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
        );

        // Return the most recent bookmarks event
        return events.sort((a, b) => b.created_at - a.created_at)[0] || null;
      } catch (error) {
        console.error('Failed to fetch bookmarks:', error);
        return null;
      }
    },
    enabled: !!userPubkey,
    staleTime: 30000,
  });

  // Parse bookmarks from the event tags
  const bookmarks: Bookmark[] = bookmarksEvent?.tags
    .map((tag): Bookmark | null => {
      if (tag[0] === 'e') {
        return { type: 'note' as const, value: tag[1], relay: tag[2] };
      } else if (tag[0] === 'a') {
        return { type: 'article' as const, value: tag[1], relay: tag[2] };
      } else if (tag[0] === 't') {
        return { type: 'hashtag' as const, value: tag[1] };
      } else if (tag[0] === 'r') {
        return { type: 'url' as const, value: tag[1] };
      }
      return null;
    })
    .filter((b): b is Bookmark => b !== null) ?? [];

  // Check if a specific note is bookmarked
  const isBookmarked = (eventId: string): boolean => {
    return bookmarks.some(b => b.type === 'note' && b.value === eventId);
  };

  // Add a bookmark
  const addBookmark = useMutation({
    mutationFn: async ({ type, value, relay }: { type: string; value: string; relay?: string }) => {
      if (!user) throw new Error('User not logged in');

      // Build new tags array with the new bookmark appended
      const existingTags = bookmarksEvent?.tags || [];
      let newTag: string[];

      if (type === 'note') {
        newTag = relay ? ['e', value, relay] : ['e', value];
      } else if (type === 'article') {
        newTag = relay ? ['a', value, relay] : ['a', value];
      } else if (type === 'hashtag') {
        newTag = ['t', value];
      } else if (type === 'url') {
        newTag = ['r', value];
      } else {
        throw new Error('Invalid bookmark type');
      }

      // Check if bookmark already exists
      const alreadyExists = existingTags.some(tag =>
        tag[0] === newTag[0] && tag[1] === newTag[1]
      );

      if (alreadyExists) {
        // Silently skip if already bookmarked
        return null;
      }

      // Create and sign the bookmarks event using user.signer
      const signedEvent = await user.signer.signEvent({
        kind: BOOKMARKS_KIND,
        created_at: Math.floor(Date.now() / 1000),
        tags: [...existingTags, newTag],
        content: bookmarksEvent?.content || '', // Preserve encrypted content if any
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['bookmarks', userPubkey] });
    },
    onError: (error) => {
      console.error('Failed to add bookmark:', error);
    },
  });

  // Remove a bookmark
  const removeBookmark = useMutation({
    mutationFn: async ({ type, value }: { type: string; value: string }) => {
      if (!user) throw new Error('User not logged in');
      if (!bookmarksEvent) {
        // No bookmarks to remove from, silently skip
        return null;
      }

      // Filter out the bookmark to remove
      const newTags = bookmarksEvent.tags.filter(tag => {
        if (type === 'note' && tag[0] === 'e') return tag[1] !== value;
        if (type === 'article' && tag[0] === 'a') return tag[1] !== value;
        if (type === 'hashtag' && tag[0] === 't') return tag[1] !== value;
        if (type === 'url' && tag[0] === 'r') return tag[1] !== value;
        return true;
      });

      // Create and sign the bookmarks event using user.signer
      const signedEvent = await user.signer.signEvent({
        kind: BOOKMARKS_KIND,
        created_at: Math.floor(Date.now() / 1000),
        tags: newTags,
        content: bookmarksEvent.content || '',
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['bookmarks', userPubkey] });
    },
    onError: (error) => {
      console.error('Failed to remove bookmark:', error);
    },
  });

  // Toggle bookmark for a note
  const toggleBookmark = useMutation({
    mutationFn: async ({ eventId, relay }: { eventId: string; relay?: string }) => {
      const currentlyBookmarked = isBookmarked(eventId);

      if (currentlyBookmarked) {
        return await removeBookmark.mutateAsync({ type: 'note', value: eventId });
      } else {
        return await addBookmark.mutateAsync({ type: 'note', value: eventId, relay });
      }
    },
  });

  return {
    bookmarks,
    bookmarksEvent,
    isLoading,
    isBookmarked,
    addBookmark,
    removeBookmark,
    toggleBookmark,
  };
}
