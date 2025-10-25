import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
import { loadWithRouter } from '$lib/services/outbox';
import { currentUser } from '$lib/stores/auth';
import { useNostrPublish } from '$lib/stores/publish.svelte';
import { get } from 'svelte/store';
import type { TrustedEvent } from '@welshman/util';

export type UserList = {
	id: string;
	kind: number;
	name: string;
	description: string;
	identifier: string;
	tags: string[][];
	created_at: number;
	author: string;
	event: TrustedEvent;
};

/**
 * Hook to fetch all user lists (NIP-51)
 * Supports:
 * - 30000: People lists
 * - 30002: Relay lists
 * - 30003: Bookmark lists
 * - 30004: Curation lists
 */
export function useUserLists() {
	return createQuery(() => ({
		queryKey: ['user-lists', get(currentUser)?.pubkey],
		queryFn: async ({ signal }) => {
			const user = get(currentUser);
			if (!user) return [];

			const events = await loadWithRouter({
				filters: [
					{
						kinds: [30000, 30002, 30003, 30004], // NIP-51 list kinds
						authors: [user.pubkey]
					}
				],
				signal,
			});

			// Parse events into UserList objects
			const lists: UserList[] = events
				.filter((event) => event.tags.some((tag) => tag[0] === 'd')) // Must have identifier
				.map((event) => {
					const dTag = event.tags.find((tag) => tag[0] === 'd');
					const titleTag = event.tags.find((tag) => tag[0] === 'title');
					const descTag = event.tags.find((tag) => tag[0] === 'description');

					return {
						id: event.id,
						kind: event.kind,
						name: titleTag?.[1] || dTag?.[1] || 'Untitled List',
						description: descTag?.[1] || '',
						identifier: dTag?.[1] || '',
						tags: event.tags,
						created_at: event.created_at,
						author: event.pubkey,
						event
					};
				})
				.sort((a, b) => b.created_at - a.created_at);

			return lists;
		},
		staleTime: 60000, // 1 minute
		enabled: !!get(currentUser)
	}));
}

/**
 * Hook to fetch a single list by identifier
 */
export function useList(kind: number, identifier: string) {
	return createQuery(() => ({
		queryKey: ['list', kind, identifier, get(currentUser)?.pubkey],
		queryFn: async ({ signal }) => {
			const user = get(currentUser);
			if (!user) return null;

			const events = await loadWithRouter({
				filters: [
					{
						kinds: [kind],
						authors: [user.pubkey],
						'#d': [identifier]
					}
				],
				signal,
			});

			if (events.length === 0) return null;

			// Get most recent
			const event = events.sort((a, b) => b.created_at - a.created_at)[0];

			const dTag = event.tags.find((tag) => tag[0] === 'd');
			const titleTag = event.tags.find((tag) => tag[0] === 'title');
			const descTag = event.tags.find((tag) => tag[0] === 'description');

			return {
				id: event.id,
				kind: event.kind,
				name: titleTag?.[1] || dTag?.[1] || 'Untitled List',
				description: descTag?.[1] || '',
				identifier: dTag?.[1] || '',
				tags: event.tags,
				created_at: event.created_at,
				author: event.pubkey,
				event
			} as UserList;
		},
		staleTime: 60000,
		enabled: !!get(currentUser) && !!identifier
	}));
}

/**
 * Hook to create or update a list
 */
export function useCreateList() {
	const queryClient = useQueryClient();
	const publish = useNostrPublish();

	return createMutation(() => ({
		mutationFn: async ({
			kind,
			identifier,
			title,
			description,
			tags
		}: {
			kind: number;
			identifier: string;
			title?: string;
			description?: string;
			tags: string[][];
		}) => {
			const user = get(currentUser);
			if (!user) throw new Error('Must be logged in to create lists');

			// Build list tags
			const listTags: string[][] = [['d', identifier]];

			if (title) {
				listTags.push(['title', title]);
			}

			if (description) {
				listTags.push(['description', description]);
			}

			// Add provided tags (e.g., p-tags, e-tags, r-tags, t-tags)
			listTags.push(...tags);

			await publish.mutateAsync({
				kind,
				content: '',
				tags: listTags
			});
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['user-lists'] });
			queryClient.invalidateQueries({ queryKey: ['list'] });
		}
	}));
}

/**
 * Hook to delete a list
 */
export function useDeleteList() {
	const queryClient = useQueryClient();
	const publish = useNostrPublish();

	return createMutation(() => ({
		mutationFn: async ({ event }: { event: TrustedEvent }) => {
			const user = get(currentUser);
			if (!user) throw new Error('Must be logged in to delete lists');

			// Publish deletion event (kind 5)
			await publish.mutateAsync({
				kind: 5,
				content: 'Deleted list',
				tags: [
					['e', event.id],
					['k', String(event.kind)]
				]
			});
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['user-lists'] });
			queryClient.invalidateQueries({ queryKey: ['list'] });
		}
	}));
}

/**
 * Get list type name from kind
 */
export function getListTypeName(kind: number): string {
	switch (kind) {
		case 30000:
			return 'People';
		case 30002:
			return 'Relays';
		case 30003:
			return 'Bookmarks';
		case 30004:
			return 'Curations';
		default:
			return 'Custom';
	}
}

/**
 * Get list icon from kind
 */
export function getListIcon(kind: number): string {
	switch (kind) {
		case 30000:
			return '👥';
		case 30002:
			return '🔗';
		case 30003:
			return '🔖';
		case 30004:
			return '📚';
		default:
			return '📋';
	}
}
