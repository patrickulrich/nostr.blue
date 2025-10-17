import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useInfiniteQuery } from '@tanstack/react-query';
import { useCurrentUser } from './useCurrentUser';

export type NotificationType = 'reply' | 'mention' | 'reaction' | 'repost' | 'zap';

export interface NotificationEvent {
  type: NotificationType;
  event: NostrEvent;
  targetEventId?: string; // The event being replied to, reacted to, etc.
}

/**
 * Hook to fetch and manage user notifications from the Nostr network.
 * Aggregates replies, mentions, reactions, reposts, and zaps with infinite scrolling.
 * @returns Infinite query result containing paginated notification events
 */
export function useNotifications() {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const userPubkey = user?.pubkey;

  return useInfiniteQuery<NotificationEvent[]>({
    queryKey: ['notifications', userPubkey],
    queryFn: async ({ pageParam = undefined, signal }) => {
      if (!userPubkey) return [];

      const limit = 50;
      const filters: Record<string, unknown> = {
        limit,
      };

      // Use pageParam (timestamp) as "until" for pagination
      if (pageParam) {
        filters.until = pageParam as number;
      }

      try {
        // Fetch all notification types in a single query for better performance and consistent pagination
        const allEvents = await nostr.query(
          [{ kinds: [1, 7, 6, 9735], '#p': [userPubkey], ...filters }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
        );

        // Split events by kind for processing
        const mentionEvents = allEvents.filter(e => e.kind === 1);
        const reactionEvents = allEvents.filter(e => e.kind === 7);
        const repostEvents = allEvents.filter(e => e.kind === 6);
        const zapEvents = allEvents.filter(e => e.kind === 9735);

        // Combine and categorize all notifications
        const notifications: NotificationEvent[] = [];

        // Process mentions
        mentionEvents.forEach(event => {
          // Check if it's a reply (has 'e' tag) or just a mention
          const replyTag = event.tags.find(tag => tag[0] === 'e');
          if (replyTag) {
            notifications.push({
              type: 'reply',
              event,
              targetEventId: replyTag[1],
            });
          } else {
            notifications.push({
              type: 'mention',
              event,
            });
          }
        });

        // Process reactions
        reactionEvents.forEach(event => {
          const eventTag = event.tags.find(tag => tag[0] === 'e');
          notifications.push({
            type: 'reaction',
            event,
            targetEventId: eventTag?.[1],
          });
        });

        // Process reposts
        repostEvents.forEach(event => {
          const eventTag = event.tags.find(tag => tag[0] === 'e');
          notifications.push({
            type: 'repost',
            event,
            targetEventId: eventTag?.[1],
          });
        });

        // Process zaps
        zapEvents.forEach(event => {
          const eventTag = event.tags.find(tag => tag[0] === 'e');
          notifications.push({
            type: 'zap',
            event,
            targetEventId: eventTag?.[1],
          });
        });

        // Sort by created_at descending (newest first)
        notifications.sort((a, b) => b.event.created_at - a.event.created_at);

        // Return latest notifications
        return notifications.slice(0, limit);
      } catch (error) {
        console.error('Notifications query error:', error);
        return [];
      }
    },
    getNextPageParam: (lastPage) => {
      if (!lastPage || lastPage.length === 0) {
        return undefined;
      }

      // Use the oldest notification's timestamp minus 1 second as the next page param
      const oldestNotification = lastPage[lastPage.length - 1];
      return oldestNotification ? oldestNotification.event.created_at - 1 : undefined;
    },
    initialPageParam: undefined,
    enabled: !!userPubkey,
    refetchOnWindowFocus: false,
    staleTime: 30000, // Consider data fresh for 30 seconds
  });
}
