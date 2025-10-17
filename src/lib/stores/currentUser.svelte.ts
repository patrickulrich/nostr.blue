import { pubkey, session } from '@welshman/app';
import { useAuthor } from './author.svelte';
import type { Profile } from '@welshman/util';
import { get } from 'svelte/store';

/**
 * Get the current logged-in user with their profile metadata
 * Combines Welshman session data with author profile query
 *
 * @returns TanStack Query with current user data including metadata
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { useCurrentUser } from '$lib/stores/currentUser.svelte';
 *
 *   const currentUser = useCurrentUser();
 *
 *   let metadata = $derived($currentUser.data?.metadata);
 *   let displayName = $derived(metadata?.name ?? 'Anonymous');
 * </script>
 *
 * {#if $pubkey}
 *   <div>Logged in as {displayName}</div>
 * {/if}
 * ```
 */
export function useCurrentUser() {
	return useAuthor(get(pubkey));
}

/**
 * Get current user's metadata directly from the query
 * Returns undefined if not logged in or metadata not loaded
 */
export function getCurrentUserMetadata(): Profile | undefined {
	const currentPubkey = get(pubkey);
	if (!currentPubkey) return undefined;

	const author = useAuthor(currentPubkey);
	return author.data?.metadata;
}
