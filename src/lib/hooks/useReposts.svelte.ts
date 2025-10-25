import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
import { loadWithRouter } from '$lib/services/outbox';
import { currentUser } from '$lib/stores/auth';
import { useNostrPublish } from '$lib/stores/publish.svelte';
import { get } from 'svelte/store';
import type { TrustedEvent } from '@welshman/util';

/**
 * Hook to manage reposts for a note
 * @param event - The event to repost
 */
export function useReposts(event: TrustedEvent) {
	const queryClient = useQueryClient();
	const publish = useNostrPublish();

	const repostsQuery = createQuery(() => ({
		queryKey: ['reposts', event.id],
		queryFn: async ({ signal }) => {
			const reposts = await loadWithRouter({
				filters: [
					{
						kinds: [6], // Reposts (NIP-18)
						'#e': [event.id]
					}
				],
				signal,
			});

			const user = get(currentUser);
			const hasReposted = reposts.some((r) => r.pubkey === user?.pubkey);

			return {
				count: reposts.length,
				hasReposted
			};
		},
		staleTime: 30000
	}));

	const repostMutation = createMutation(() => ({
		mutationFn: async () => {
			const user = get(currentUser);
			if (!user) {
				throw new Error('Must be logged in to repost');
			}

			await publish.mutateAsync({
				kind: 6,
				content: JSON.stringify(event),
				tags: [
					['e', event.id],
					['p', event.pubkey]
				]
			});
		},
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['reposts', event.id] });
		}
	}));

	return {
		reposts: repostsQuery.data,
		isLoading: repostsQuery.isLoading,
		repost: repostMutation
	};
}
