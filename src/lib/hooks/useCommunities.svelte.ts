import type { TrustedEvent } from '@welshman/util';
import { createQuery } from '@tanstack/svelte-query';
import { loadWithRouter } from '$lib/services/outbox';
import { currentUser } from '$lib/stores/auth';
import { get } from 'svelte/store';

export interface Community {
	id: string;
	pubkey: string;
	dTag: string; // community identifier
	name?: string;
	description?: string;
	image?: string;
	moderators: string[];
	relays: string[];
	event: TrustedEvent;
	aTag: string; // formatted as "34550:pubkey:dTag"
}

/**
 * Hook to fetch all available communities from the Nostr network.
 * Queries for kind 34550 events (NIP-72 community definitions) and parses their metadata.
 */
export function useCommunities() {
	return createQuery<Community[]>(() => ({
		queryKey: ['communities'],
		queryFn: async ({ signal }) => {
			try {
				const events = await loadWithRouter({
					filters: [{ kinds: [34550], limit: 100 }],
					signal
				});

				const communities: Community[] = events.map((event) => {
					const dTag = event.tags.find((t) => t[0] === 'd')?.[1] || '';
					const name = event.tags.find((t) => t[0] === 'name')?.[1];
					const description = event.tags.find((t) => t[0] === 'description')?.[1];
					const image = event.tags.find((t) => t[0] === 'image')?.[1];
					const moderators = event.tags
						.filter((t) => t[0] === 'p' && t[3] === 'moderator')
						.map((t) => t[1]);
					const relays = event.tags.filter((t) => t[0] === 'relay').map((t) => t[1]);

					return {
						id: event.id,
						pubkey: event.pubkey,
						dTag,
						name: name || dTag,
						description,
						image,
						moderators,
						relays,
						event,
						aTag: `34550:${event.pubkey}:${dTag}`
					};
				});

				return communities.sort((a, b) => b.event.created_at - a.event.created_at);
			} catch (error) {
				console.error('Failed to fetch communities:', error);
				return [];
			}
		},
		staleTime: 60000 // 1 minute
	}));
}

/**
 * Hook to fetch a specific community by its NIP-19 address tag.
 * Queries for the community definition event and parses its metadata.
 * @param aTag - The community address tag in format "34550:pubkey:dTag"
 */
export function useCommunity(aTag: string) {
	// Parse aTag: "34550:pubkey:dTag"
	const [_kind, pubkey, dTag] = aTag.split(':');

	return createQuery<Community | null>(() => ({
		queryKey: ['community', aTag],
		queryFn: async ({ signal }) => {
			if (!pubkey || !dTag) return null;

			try {
				const events = await loadWithRouter({
					filters: [{ kinds: [34550], authors: [pubkey], '#d': [dTag], limit: 1 }],
					signal
				});

				const event = events[0];
				if (!event) return null;

				const name = event.tags.find((t) => t[0] === 'name')?.[1];
				const description = event.tags.find((t) => t[0] === 'description')?.[1];
				const image = event.tags.find((t) => t[0] === 'image')?.[1];
				const moderators = event.tags
					.filter((t) => t[0] === 'p' && t[3] === 'moderator')
					.map((t) => t[1]);
				const relays = event.tags.filter((t) => t[0] === 'relay').map((t) => t[1]);

				return {
					id: event.id,
					pubkey: event.pubkey,
					dTag,
					name: name || dTag,
					description,
					image,
					moderators,
					relays,
					event,
					aTag
				};
			} catch (error) {
				console.error('Failed to fetch community:', error);
				return null;
			}
		},
		enabled: !!pubkey && !!dTag,
		staleTime: 60000
	}));
}

/**
 * Hook to fetch all posts belonging to a specific community.
 * Fetches kind 1111 (NIP-72 community posts) and kind 1 posts tagged with the community address.
 * @param aTag - The community address tag in format "34550:pubkey:dTag"
 */
export function useCommunityPosts(aTag: string) {
	// Parse aTag: "34550:pubkey:dTag"
	const [_kind, pubkey, dTag] = aTag.split(':');

	return createQuery<TrustedEvent[]>(() => ({
		queryKey: ['community-posts', aTag],
		queryFn: async ({ signal }) => {
			if (!pubkey || !dTag) return [];

			try {
				// Fetch kind 1111 posts (NIP-72 standard) and kind 1 posts (backwards compatibility)
				const events = await loadWithRouter({
					filters: [
						{ kinds: [1111], '#a': [aTag], limit: 100 },
						{ kinds: [1], '#a': [aTag], limit: 100 }
					],
					signal,
				});

				return events.sort((a, b) => b.created_at - a.created_at);
			} catch (error) {
				console.error('Failed to fetch community posts:', error);
				return [];
			}
		},
		enabled: !!pubkey && !!dTag,
		staleTime: 30000
	}));
}

/**
 * Hook to get communities the current user is a member of.
 * Determines membership by checking which communities the user has posted to.
 */
export function useUserCommunities() {
	return createQuery<Set<string>>(() => {
		const user = get(currentUser);

		return {
			queryKey: ['user-communities', user?.pubkey],
			queryFn: async ({ signal }) => {
				if (!user) return new Set<string>();

				try {
					// Fetch user's posts that have community tags (kinds 1111 and 1)
					const events = await loadWithRouter({
						filters: [{ kinds: [1111, 1], authors: [user.pubkey], limit: 500 }],
						signal,
					});

					// Extract all community a tags from user's posts
					const communityATags = new Set<string>();
					events.forEach((event) => {
						event.tags.forEach((tag) => {
							if (tag[0] === 'a' && tag[1]?.startsWith('34550:')) {
								communityATags.add(tag[1]);
							}
						});
					});

					return communityATags;
				} catch (error) {
					console.error('Failed to fetch user communities:', error);
					return new Set<string>();
				}
			},
			enabled: !!user,
			staleTime: 60000
		};
	});
}
