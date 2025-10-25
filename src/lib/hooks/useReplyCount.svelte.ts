import { createQuery } from '@tanstack/svelte-query';
import { loadWithRouter } from '$lib/services/outbox';

/**
 * Hook to fetch reply count for a note
 * @param eventId - The event ID to count replies for
 */
export function useReplyCount(eventId: string) {
	return createQuery(() => ({
		queryKey: ['reply-count', eventId],
		queryFn: async ({ signal }) => {
			const replies = await loadWithRouter({
				filters: [
					{
						kinds: [1],
						'#e': [eventId],
						limit: 500 // Fetch up to 500 to get accurate count
					}
				],
				signal,
			});

			return replies.length;
		},
		staleTime: 30000, // Cache for 30 seconds
		gcTime: 60000
	}));
}
