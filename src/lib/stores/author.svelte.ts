import type { TrustedEvent, Profile } from '@welshman/util';
import { PROFILE } from '@welshman/util';
import { createQuery } from '@tanstack/svelte-query';
import { load } from '@welshman/net';

/**
 * Query author profile by pubkey
 * Returns a TanStack Query for the author's kind 0 profile event and parsed metadata
 *
 * @param pubkey - The public key of the author to fetch
 * @returns TanStack Query with author data
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   const author = useAuthor(event.pubkey);
 *   let metadata = $derived($author.data?.metadata);
 *   let displayName = $derived(metadata?.name ?? metadata?.display_name ?? 'Anonymous');
 * </script>
 *
 * {#if $author.data?.metadata}
 *   <div>{displayName}</div>
 * {/if}
 * ```
 */
export function useAuthor(pubkey: string | undefined) {
	return createQuery(() => ({
		queryKey: ['author', pubkey ?? ''],
		queryFn: async ({ signal }) => {
			if (!pubkey) {
				return {};
			}

			const events = await load({
				relays: [], // Will use default relays from router
				filters: [{ kinds: [PROFILE], authors: [pubkey], limit: 1 }],
				signal,
				timeout: 1500
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
		},
		retry: 3,
		enabled: !!pubkey
	}));
}

export type AuthorData = {
	event?: TrustedEvent;
	metadata?: Profile;
};
