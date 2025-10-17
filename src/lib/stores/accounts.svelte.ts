import type { TrustedEvent, Profile } from '@welshman/util';
import { PROFILE } from '@welshman/util';
import { createQuery } from '@tanstack/svelte-query';
import { load } from '@welshman/net';
import { sessions, pubkey } from '@welshman/app';
import { derived, get } from 'svelte/store';

export interface Account {
	pubkey: string;
	event?: TrustedEvent;
	metadata: Profile;
}

/**
 * Query all logged-in accounts with their profile metadata
 * Returns a TanStack Query with account data for all sessions
 *
 * @returns TanStack Query with accounts data and utility functions
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { useLoggedInAccounts } from '$lib/stores/accounts.svelte';
 *
 *   const accounts = useLoggedInAccounts();
 *
 *   let currentUser = $derived($accounts.data?.currentUser);
 *   let otherUsers = $derived($accounts.data?.otherUsers ?? []);
 * </script>
 *
 * {#if currentUser}
 *   <div>Current: {currentUser.metadata.name ?? 'Anonymous'}</div>
 * {/if}
 *
 * {#each otherUsers as user}
 *   <div>{user.metadata.name ?? 'Anonymous'}</div>
 * {/each}
 * ```
 */
export function useLoggedInAccounts() {
	const allSessions = derived(sessions, ($sessions) => Object.values($sessions));

	return createQuery(() => {
		const $sessions = get(allSessions);
		const currentPubkey = get(pubkey);

		return {
			queryKey: ['accounts', $sessions.map((s) => s.pubkey).join(';')],
			queryFn: async ({ signal }) => {
				if ($sessions.length === 0) {
					return { authors: [], currentUser: undefined, otherUsers: [] };
				}

				const pubkeys = $sessions.map((s) => s.pubkey);

				const events = await load({
					relays: [],
					filters: [{ kinds: [PROFILE], authors: pubkeys }],
					signal,
					timeout: 1500
				});

				const authors: Account[] = pubkeys.map((pk) => {
					const event = events.find((e) => e.pubkey === pk);
					try {
						const metadata = event?.content ? JSON.parse(event.content) : {};
						return { pubkey: pk, metadata, event };
					} catch {
						return { pubkey: pk, metadata: {}, event };
					}
				});

				// Current user is the first one matching the active pubkey
				const currentUser: Account | undefined = authors.find(
					(a) => a.pubkey === currentPubkey
				);

				// Other users are all accounts except the current one
				const otherUsers = authors.filter((a) => a.pubkey !== currentPubkey);

				return {
					authors,
					currentUser,
					otherUsers
				};
			},
			retry: 3
		};
	});
}

/**
 * Derived store for the current user account
 */
export const currentUserAccount = derived(
	[sessions, pubkey],
	([$sessions, $pubkey]) => {
		if (!$pubkey) return undefined;
		return $sessions[$pubkey];
	}
);

/**
 * Derived store for other user accounts (not the current one)
 */
export const otherUserAccounts = derived(
	[sessions, pubkey],
	([$sessions, $pubkey]) => {
		return Object.values($sessions).filter((s) => s.pubkey !== $pubkey);
	}
);
