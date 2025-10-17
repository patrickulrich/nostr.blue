import { useQuery } from '@tanstack/react-query';
import { useNostr } from '@nostrify/react';

/**
 * Hook to get the count of replies to a Nostr event
 * For kind 1 posts, replies are kind 1 events with an 'e' tag referencing the post
 */
export function useReplyCount(eventId: string | undefined) {
  const { nostr } = useNostr();

  return useQuery<number>({
    queryKey: ['reply-count', eventId],
    queryFn: async ({ signal }) => {
      if (!eventId) {
        return 0;
      }

      try {
        const timeoutSignal = AbortSignal.timeout(3000);
        const combinedSignal = AbortSignal.any([signal, timeoutSignal]);

        // Query for kind 1 events that have an 'e' tag referencing this event
        const replies = await nostr.query(
          [
            {
              kinds: [1],
              '#e': [eventId],
              limit: 500, // Limit to prevent too many results
            },
          ],
          { signal: combinedSignal }
        );

        return replies.length;
      } catch (error) {
        // Don't log AbortError as it's expected when queries are cancelled
        if (error instanceof Error && error.name === 'AbortError') {
          return 0;
        }
        console.error('Failed to fetch reply count:', error);
        return 0;
      }
    },
    enabled: !!eventId,
    staleTime: 30 * 1000, // Cache for 30 seconds
    gcTime: 5 * 60 * 1000, // Keep in cache for 5 minutes
    retry: 1,
  });
}
