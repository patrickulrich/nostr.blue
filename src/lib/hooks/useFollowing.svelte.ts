import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
import { currentPubkey } from '$lib/stores/auth';
import { getContactList, extractFollowing, followUser, unfollowUser } from '$lib/stores/following.svelte';
import { get } from 'svelte/store';

/**
 * Hook to manage following/contact lists (Kind 3)
 * @param pubkey - Optional pubkey to fetch following list for. Defaults to current user.
 */
export function useFollowing(pubkey?: string) {
	const queryClient = useQueryClient();

	// Use provided pubkey or current user's pubkey
	const targetPubkey = $derived(pubkey || get(currentPubkey));

	// Query the contact list
	const contactQuery = createQuery(() => ({
		queryKey: ['contacts', targetPubkey],
		queryFn: async ({ signal: _signal }) => {
			if (!targetPubkey) return null;

			const contactEvent = await getContactList(targetPubkey);
			return contactEvent;
		},
		enabled: !!targetPubkey,
		staleTime: 60000, // Cache for 1 minute
		signal: undefined
	}));

	// Extract following list from contact event
	const following = $derived.by(() => {
		const contactEvent = contactQuery.data;
		return extractFollowing(contactEvent ?? null);
	});

	// Check if current user is following a specific pubkey
	function isFollowing(checkPubkey: string): boolean {
		return following.includes(checkPubkey);
	}

	// Mutation to follow a user
	const followMutation = createMutation(() => ({
		mutationFn: async (followPubkey: string) => {
			return await followUser(followPubkey);
		},
		onSuccess: () => {
			const userPubkey = get(currentPubkey);
			queryClient.invalidateQueries({ queryKey: ['contacts', userPubkey] });
		}
	}));

	// Mutation to unfollow a user
	const unfollowMutation = createMutation(() => ({
		mutationFn: async (unfollowPubkey: string) => {
			return await unfollowUser(unfollowPubkey);
		},
		onSuccess: () => {
			const userPubkey = get(currentPubkey);
			queryClient.invalidateQueries({ queryKey: ['contacts', userPubkey] });
		}
	}));

	return {
		get following() {
			return following;
		},
		get followingCount() {
			return following.length;
		},
		isFollowing,
		isLoading: contactQuery.isLoading,
		follow: followMutation,
		unfollow: unfollowMutation
	};
}
