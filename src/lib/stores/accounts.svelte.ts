import type { TrustedEvent, Profile } from '@welshman/util';
import { PROFILE } from '@welshman/util';
import { load } from '@welshman/net';
import { sessions, pubkey } from '@welshman/app';
import { derived, get } from 'svelte/store';

export interface Account {
	pubkey: string;
	event?: TrustedEvent;
	metadata: Profile;
}

export interface AccountsData {
	authors: Account[];
	currentUser: Account | undefined;
	otherUsers: Account[];
}

/**
 * Fetch all logged-in accounts with their profile metadata
 * Use this with createQuery directly in components
 *
 * @param sessionPubkeys - Array of pubkeys from active sessions
 * @param currentPubkey - The currently active pubkey
 * @param signal - Optional abort signal for request cancellation
 * @returns Accounts data with current user and other users
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { createQuery } from '@tanstack/svelte-query';
 *   import { fetchAccounts, type AccountsData } from '$lib/stores/accounts.svelte';
 *   import { sessions, pubkey } from '@welshman/app';
 *   import { get } from 'svelte/store';
 *
 *   const sessionPubkeys = $derived(Object.keys(get(sessions)));
 *   const currentPubkey = $derived(get(pubkey));
 *
 *   const accountsQuery = createQuery<AccountsData>(() => ({
 *     queryKey: ['accounts', sessionPubkeys.join(';')],
 *     queryFn: ({ signal }) => fetchAccounts(sessionPubkeys, currentPubkey, signal)
 *   }));
 *
 *   let currentUser = $derived($accountsQuery.data?.currentUser);
 *   let otherUsers = $derived($accountsQuery.data?.otherUsers ?? []);
 * </script>
 * ```
 */
export async function fetchAccounts(
	sessionPubkeys: string[],
	currentPubkey: string | undefined,
	signal?: AbortSignal
): Promise<AccountsData> {
	if (sessionPubkeys.length === 0) {
		return { authors: [], currentUser: undefined, otherUsers: [] };
	}

	const events = await load({
		relays: [],
		filters: [{ kinds: [PROFILE], authors: sessionPubkeys }],
		signal,
	});

	const authors: Account[] = sessionPubkeys.map((pk) => {
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
