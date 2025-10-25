import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
import { loadWithRouter } from '$lib/services/outbox';
import { currentUser } from '$lib/stores/auth';
import { useNostrPublish } from '$lib/stores/publish.svelte';
import { get } from 'svelte/store';

/**
 * Hook to manage bookmarks using NIP-51 lists (kind 30001)
 * @param eventId - The event ID to bookmark
 */
export function useBookmarks(eventId: string) {
	const queryClient = useQueryClient();
	const publish = useNostrPublish();

	// Fetch user's bookmark list
	const bookmarksQuery = createQuery(() => ({
		queryKey: ['bookmarks', get(currentUser)?.pubkey],
		queryFn: async ({ signal }) => {
			const user = get(currentUser);
			if (!user) return { isBookmarked: false, bookmarkList: null };

			const bookmarkLists = await loadWithRouter({
				filters: [
					{
						kinds: [30001], // NIP-51 bookmark list
						authors: [user.pubkey],
						'#d': ['bookmarks']
					}
				],
				signal
			});

			// Get the most recent bookmark list
			const bookmarkList = bookmarkLists.sort((a, b) => b.created_at - a.created_at)[0];

			if (!bookmarkList) return { isBookmarked: false, bookmarkList: null };

			// Check if this event is bookmarked
			const bookmarkedEventIds = bookmarkList.tags
				.filter((tag) => tag[0] === 'e')
				.map((tag) => tag[1]);

			return {
				isBookmarked: bookmarkedEventIds.includes(eventId),
				bookmarkList
			};
		},
		staleTime: 10000,
		enabled: !!get(currentUser)
	}));

	// Toggle bookmark mutation
	const toggleBookmarkMutation = createMutation(() => ({
		mutationFn: async () => {
			const user = get(currentUser);
			if (!user) {
				throw new Error('Must be logged in to bookmark');
			}

			const currentData = bookmarksQuery.data;
			let tags: string[][] = [];

			if (currentData?.bookmarkList) {
				// Get existing bookmarks
				tags = currentData.bookmarkList.tags.filter((tag) => tag[0] === 'e' || tag[0] === 'd');

				// Toggle: remove if bookmarked, add if not
				if (currentData.isBookmarked) {
					tags = tags.filter((tag) => !(tag[0] === 'e' && tag[1] === eventId));
				} else {
					tags.push(['e', eventId]);
				}
			} else {
				// Create new bookmark list
				tags = [
					['d', 'bookmarks'],
					['e', eventId]
				];
			}

			// Ensure 'd' tag is present
			if (!tags.some((tag) => tag[0] === 'd')) {
				tags.unshift(['d', 'bookmarks']);
			}

			await publish.mutateAsync({
				kind: 30001,
				content: '',
				tags
			});
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['bookmarks'] });
		}
	}));

	return {
		isBookmarked: bookmarksQuery.data?.isBookmarked || false,
		isLoading: bookmarksQuery.isLoading,
		toggleBookmark: toggleBookmarkMutation
	};
}
