import type { TrustedEvent, Profile } from '@welshman/util';
import { PROFILE } from '@welshman/util';
import { load } from '@welshman/net';

/**
 * Fetch author profile by pubkey
 * Use this with createQuery directly in components
 *
 * @param pubkey - The public key of the author to fetch
 * @param signal - Optional abort signal for request cancellation
 * @returns Author data with metadata and event
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { createQuery } from '@tanstack/svelte-query';
 *   import { fetchAuthor, type AuthorData } from '$lib/stores/author.svelte';
 *
 *   const authorQuery = createQuery<AuthorData>(() => ({
 *     queryKey: ['author', event.pubkey],
 *     queryFn: ({ signal }) => fetchAuthor(event.pubkey, signal),
 *     enabled: !!event.pubkey
 *   }));
 *
 *   let metadata = $derived($authorQuery.data?.metadata);
 *   let displayName = $derived(metadata?.name ?? metadata?.display_name ?? 'Anonymous');
 * </script>
 * ```
 */
export async function fetchAuthor(
	pubkey: string | undefined,
	signal?: AbortSignal
): Promise<AuthorData> {
	if (!pubkey) {
		return {};
	}

	const events = await load({
		relays: [], // Will use default relays from router
		filters: [{ kinds: [PROFILE], authors: [pubkey], limit: 1 }],
		signal,
	});

	const event = events[0];

	if (!event) {
		return {};
	}

	try {
		const metadata = JSON.parse(event.content) as Profile;
		return { metadata, event };
	} catch {
		return { event };
	}
}

export type AuthorData = {
	event?: TrustedEvent;
	metadata?: Profile;
};
