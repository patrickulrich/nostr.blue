import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
import { loadWithRouter } from '$lib/services/outbox';
import { currentUser } from '$lib/stores/auth';
import { useNostrPublish } from '$lib/stores/publish.svelte';
import { get } from 'svelte/store';

/**
 * Hook to manage reactions for a note
 * @param eventId - The event ID
 * @param authorPubkey - The event author's pubkey
 */
export function useReactions(eventId: string, authorPubkey: string) {
	const queryClient = useQueryClient();
	const publish = useNostrPublish();

	const reactionsQuery = createQuery(() => ({
		queryKey: ['reactions', eventId],
		queryFn: async ({ signal }) => {
			const reactions = await loadWithRouter({
				filters: [
					{
						kinds: [7], // Reactions (NIP-25)
						'#e': [eventId]
					}
				],
				signal,
			});

			const likes = reactions.filter((r) => r.content === '+' || r.content === '❤️');
			const user = get(currentUser);
			const hasReacted = likes.some((r) => r.pubkey === user?.pubkey);

			return {
				count: likes.length,
				hasReacted
			};
		},
		staleTime: 30000
	}));

	const reactMutation = createMutation(() => ({
		mutationFn: async () => {
			const user = get(currentUser);
			if (!user) {
				throw new Error('Must be logged in to react');
			}

			await publish.mutateAsync({
				kind: 7,
				content: '+',
				tags: [
					['e', eventId],
					['p', authorPubkey]
				]
			});
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['reactions', eventId] });
		}
	}));

	return {
		reactions: reactionsQuery.data,
		isLoading: reactionsQuery.isLoading,
		react: reactMutation
	};
}
