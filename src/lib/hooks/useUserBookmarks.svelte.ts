import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
import { loadWithRouter } from '$lib/services/outbox';
import { currentUser } from '$lib/stores/auth';
import { useNostrPublish } from '$lib/stores/publish.svelte';
import { get } from 'svelte/store';
import type { TrustedEvent as _TrustedEvent } from '@welshman/util';

const BOOKMARKS_KIND = 10003;

export type Bookmark = {
	type: 'note' | 'article' | 'hashtag' | 'url';
	value: string;
	relay?: string;
};

/**
 * Hook to fetch and manage all of the current user's bookmarks (NIP-51 kind 10003)
 * Supports multiple bookmark types: notes (e), articles (a), hashtags (t), URLs (r)
 */
export function useUserBookmarks() {
	const queryClient = useQueryClient();
	const publish = useNostrPublish();

	// Fetch bookmarks list
	const bookmarksQuery = createQuery(() => ({
		queryKey: ['user-bookmarks', get(currentUser)?.pubkey],
		queryFn: async ({ signal }) => {
			const user = get(currentUser);
			if (!user) return { bookmarks: [], bookmarksEvent: null };

			const events = await loadWithRouter({
				filters: [
					{
						kinds: [BOOKMARKS_KIND],
						authors: [user.pubkey],
						limit: 1
					}
				],
				signal
			});

			// Get the most recent bookmarks event
			const bookmarksEvent = events.sort((a, b) => b.created_at - a.created_at)[0] || null;

			if (!bookmarksEvent) return { bookmarks: [], bookmarksEvent: null };

			// Parse bookmarks from tags
			const bookmarks: Bookmark[] = bookmarksEvent.tags
				.map((tag): Bookmark | null => {
					if (tag[0] === 'e') {
						return { type: 'note', value: tag[1], relay: tag[2] };
					} else if (tag[0] === 'a') {
						return { type: 'article', value: tag[1], relay: tag[2] };
					} else if (tag[0] === 't') {
						return { type: 'hashtag', value: tag[1] };
					} else if (tag[0] === 'r') {
						return { type: 'url', value: tag[1] };
					}
					return null;
				})
				.filter((b): b is Bookmark => b !== null);

			return { bookmarks, bookmarksEvent };
		},
		staleTime: 30000,
		enabled: !!get(currentUser)
	}));

	// Add bookmark mutation
	const addBookmark = createMutation(() => ({
		mutationFn: async ({
			type,
			value,
			relay
		}: {
			type: string;
			value: string;
			relay?: string;
		}) => {
			const user = get(currentUser);
			if (!user) throw new Error('User not logged in');

			const data = bookmarksQuery.data;
			const existingTags = data?.bookmarksEvent?.tags || [];

			// Build new tag
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

			// Check if already exists
			const alreadyExists = existingTags.some((tag) => tag[0] === newTag[0] && tag[1] === newTag[1]);
			if (alreadyExists) return null;

			// Publish updated bookmarks
			await publish.mutateAsync({
				kind: BOOKMARKS_KIND,
				content: data?.bookmarksEvent?.content || '',
				tags: [...existingTags, newTag]
			});
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['user-bookmarks'] });
		}
	}));

	// Remove bookmark mutation
	const removeBookmark = createMutation(() => ({
		mutationFn: async ({ type, value }: { type: string; value: string }) => {
			const user = get(currentUser);
			if (!user) throw new Error('User not logged in');

			const data = bookmarksQuery.data;
			if (!data?.bookmarksEvent) return null;

			// Filter out the bookmark to remove
			const newTags = data.bookmarksEvent.tags.filter((tag) => {
				if (type === 'note' && tag[0] === 'e') return tag[1] !== value;
				if (type === 'article' && tag[0] === 'a') return tag[1] !== value;
				if (type === 'hashtag' && tag[0] === 't') return tag[1] !== value;
				if (type === 'url' && tag[0] === 'r') return tag[1] !== value;
				return true;
			});

			// Publish updated bookmarks
			await publish.mutateAsync({
				kind: BOOKMARKS_KIND,
				content: data.bookmarksEvent.content || '',
				tags: newTags
			});
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['user-bookmarks'] });
		}
	}));

	return {
		bookmarks: bookmarksQuery.data?.bookmarks || [],
		bookmarksEvent: bookmarksQuery.data?.bookmarksEvent || null,
		isLoading: bookmarksQuery.isLoading,
		isError: bookmarksQuery.isError,
		error: bookmarksQuery.error,
		addBookmark,
		removeBookmark
	};
}

/**
 * Hook to check if a specific note is bookmarked
 */
export function useIsBookmarked(eventId: string) {
	const { bookmarks } = useUserBookmarks();
	return bookmarks.some((b) => b.type === 'note' && b.value === eventId);
}
