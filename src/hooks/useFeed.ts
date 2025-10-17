import { type NostrEvent, type NostrFilter } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useInfiniteQuery } from '@tanstack/react-query';

export interface UseFeedOptions {
  authors?: string[]; // Filter by specific authors
  kinds?: number[]; // Filter by event kinds (default: [1])
  limit?: number; // Events per page (default: 20)
  hashtag?: string; // Filter by hashtag
  excludeReplies?: boolean; // Exclude replies (top-level posts only)
}

/**
 * Hook for fetching an infinite-scrolling feed of Nostr events.
 * Supports filtering by authors, kinds, hashtags, and optionally excluding replies.
 *
 * @param options - Feed configuration options
 * @returns Infinite query result with paginated events
 */
export function useFeed(options: UseFeedOptions = {}) {
  const { nostr } = useNostr();
  const {
    authors,
    kinds = [1], // Default to Kind 1 (short text notes)
    limit = 20,
    hashtag,
    excludeReplies = false,
  } = options;

  return useInfiniteQuery<NostrEvent[]>({
    queryKey: ['feed', { authors, kinds, limit, hashtag, excludeReplies }],
    queryFn: async ({ pageParam = undefined, signal }) => {
      // Fetch more events when filtering or when we have author constraints
      const multiplier = excludeReplies ? 4 : (authors && authors.length > 0 ? 2 : 1);

      const filters: NostrFilter = {
        kinds,
        limit: limit * multiplier,
      };

      if (authors && authors.length > 0) {
        filters.authors = authors;
      }

      if (hashtag) {
        filters['#t'] = [hashtag.toLowerCase()];
      }

      // Use pageParam (timestamp) as "until" for pagination
      if (pageParam) {
        filters.until = pageParam as number;
      }

      try {
        let events = await nostr.query(
          [filters],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
        );

        // Sort by created_at descending (newest first)
        events = events.sort((a, b) => b.created_at - a.created_at);

        // Store raw event count before filtering
        const rawEventCount = events.length;

        // Filter out replies if requested
        if (excludeReplies) {
          events = events.filter(event => {
            // A post is a reply if it has an 'e' tag (references another event)
            return !event.tags.some(tag => tag[0] === 'e');
          });
        }

        // Return up to limit events
        const resultEvents = events.slice(0, limit);

        // Store metadata for pagination decision
        // Continue if we got ANY events in the raw query
        (resultEvents as NostrEvent[] & { __rawCount?: number }).__rawCount = rawEventCount;

        return resultEvents;
      } catch (error) {
        console.error('Feed query error:', error);
        return [];
      }
    },
    getNextPageParam: (lastPage) => {
      if (!lastPage || lastPage.length === 0) {
        return undefined;
      }

      // Get the raw count of events before filtering
      const rawCount = (lastPage as NostrEvent[] & { __rawCount?: number }).__rawCount || 0;

      // Continue pagination if:
      // 1. We got some raw events from the relay, OR
      // 2. We have a decent number of filtered results
      const shouldContinue = rawCount > 0 || lastPage.length >= Math.floor(limit * 0.5);

      if (!shouldContinue) {
        return undefined;
      }

      // Use the oldest event's timestamp minus 1 second as the next page param
      const oldestEvent = lastPage[lastPage.length - 1];
      return oldestEvent ? oldestEvent.created_at - 1 : undefined;
    },
    initialPageParam: undefined,
    refetchOnWindowFocus: false,
    refetchOnMount: true, // Always refetch when component mounts
    staleTime: 30000, // Consider data fresh for 30 seconds
  });
}
