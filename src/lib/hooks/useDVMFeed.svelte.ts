import { currentPubkey } from '$lib/stores/auth';
import { get } from 'svelte/store';
import type { TrustedEvent } from '@welshman/util';
import { requestDVM as customRequestDVM } from '$lib/services/dvm';

/**
 * Hook to create a DVM-powered feed with persistent subscriptions
 *
 * This uses a custom DVM implementation that keeps subscriptions open
 * for async DVM responses, bypassing welshman's autoClose issue.
 *
 * @param dvmPubkey - The DVM's pubkey to request from
 * @param requestKind - The kind of DVM request (5000-5999)
 * @param options - Additional DVM request options
 */
export function useDVMFeed(
	dvmPubkey: string,
	requestKind: number,
	options: {
		limit?: number;
		params?: Record<string, string>;
		onEvent?: (event: TrustedEvent) => void;
		onExhausted?: () => void;
		signal?: AbortSignal;
	} = {}
) {
	const { limit = 50, params = {}, onEvent, onExhausted: _onExhausted, signal } = options;

	// Get current user info
	const userPubkey = get(currentPubkey);

	if (!userPubkey) {
		throw new Error('User must be logged in to use DVM feeds');
	}

	let cleanup: (() => void) | null = null;

	return {
		/**
		 * Start the DVM request and listen for responses
		 * @param loadLimit - Number of events to request
		 */
		load: async (loadLimit: number = limit) => {
			// Cleanup previous subscription before creating new one
			if (cleanup) {
				cleanup();
				cleanup = null;
			}

			// Use our custom DVM implementation with max_results
			cleanup = await customRequestDVM({
				dvmPubkey,
				requestKind,
				params: {
					...params,
					max_results: String(loadLimit)
				},
				signal,
				onEvent: (event) => {
					onEvent?.(event);
				},
				onResponse: (_event) => {
					// DVM response received
				}
			});
		},

		/**
		 * Start listening for new events in real-time
		 * For our custom implementation, this is the same as load()
		 * @returns Cleanup function to stop listening
		 */
		listen: async (loadLimit: number = limit) => {
			// Cleanup previous subscription before creating new one
			if (cleanup) {
				cleanup();
				cleanup = null;
			}

			// Use our custom DVM implementation with max_results
			cleanup = await customRequestDVM({
				dvmPubkey,
				requestKind,
				params: {
					...params,
					max_results: String(loadLimit)
				},
				signal,
				onEvent: (event) => {
					onEvent?.(event);
				},
				onResponse: () => {
					// DVM responded
				}
			});

			return () => {
				if (cleanup) {
					cleanup();
				}
			};
		}
	};
}

/**
 * Svelte 5 reactive version of useDVMFeed for use in components
 *
 * Usage:
 * ```svelte
 * <script>
 *   const events = $state<TrustedEvent[]>([]);
 *   const feed = useDVMFeedReactive('dvmPubkey', 5300, {
 *     onEvent: (e) => events.push(e)
 *   });
 *
 *   $effect(() => {
 *     feed.load(50);
 *   });
 * </script>
 * ```
 */
export function useDVMFeedReactive(
	dvmPubkey: string,
	requestKind: number,
	options: {
		limit?: number;
		params?: Record<string, string>;
		signal?: AbortSignal;
	} = {}
) {
	const events = $state<TrustedEvent[]>([]);
	let isLoading = $state(false);
	let isExhausted = $state(false);
	let error = $state<Error | null>(null);

	const feed = useDVMFeed(dvmPubkey, requestKind, {
		...options,
		onEvent: (event) => {
			// Deduplicate
			if (!events.some((e) => e.id === event.id)) {
				events.push(event);
			}
		},
		onExhausted: () => {
			isExhausted = true;
			isLoading = false;
		}
	});

	const load = async (loadLimit?: number) => {
		isLoading = true;
		error = null;
		try {
			await feed.load(loadLimit);
		} catch (e) {
			const errorMsg = e instanceof Error ? e.message : String(e);
			error = e instanceof Error ? e : new Error(errorMsg);
		} finally {
			isLoading = false;
		}
	};

	return {
		get events() {
			return events;
		},
		get isLoading() {
			return isLoading;
		},
		get isExhausted() {
			return isExhausted;
		},
		get error() {
			return error;
		},
		load,
		listen: feed.listen
	};
}
