import { deriveProfile } from '@welshman/app';
import { currentUser } from '$lib/stores/auth';
import { derived, get } from 'svelte/store';

/**
 * Hook to get the current logged-in user's profile metadata.
 * Uses Welshman's deriveProfile to get a reactive profile store.
 */
export function useCurrentUserProfile() {
	return derived(currentUser, ($currentUser) => {
		if (!$currentUser?.pubkey) return null;

		// Create a derived store for the profile
		const profileStore = deriveProfile($currentUser.pubkey);

		// Get the current value
		return get(profileStore);
	});
}
