import { pubkey } from '@welshman/app';
import { fetchAuthor } from './author.svelte';
import type { Profile } from '@welshman/util';
import { get } from 'svelte/store';

/**
 * Get current user's metadata directly
 * Returns undefined if not logged in or if fetch fails
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { createQuery } from '@tanstack/svelte-query';
 *   import { fetchAuthor, type AuthorData } from '$lib/stores/author.svelte';
 *   import { pubkey } from '@welshman/app';
 *
 *   const currentUserQuery = createQuery<AuthorData>(() => ({
 *     queryKey: ['author', $pubkey],
 *     queryFn: ({ signal }) => fetchAuthor($pubkey, signal),
 *     enabled: !!$pubkey
 *   }));
 *
 *   let metadata = $derived($currentUserQuery.data?.metadata);
 *   let displayName = $derived(metadata?.name ?? 'Anonymous');
 * </script>
 * ```
 */
export async function getCurrentUserMetadata(): Promise<Profile | undefined> {
	const currentPubkey = get(pubkey);
	if (!currentPubkey) return undefined;

	const author = await fetchAuthor(currentPubkey);
	return author.metadata;
}
