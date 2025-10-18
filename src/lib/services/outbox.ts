/**
 * Outbox Model Service
 *
 * Provides intelligent relay selection using Welshman's router and NIP-65 relay lists.
 * Automatically queries authors' write relays instead of using hardcoded relay lists.
 */

import { load, request, type Tracker } from '@welshman/net';
import { getFilterSelections, addMinimalFallbacks, routerContext } from '@welshman/router';
import type { Filter, TrustedEvent } from '@welshman/util';
import { presetRelays } from '$lib/stores/appStore';

export type LoadWithRouterOptions = {
	/**
	 * Filters to query
	 */
	filters: Filter[];

	/**
	 * Optional signal for request cancellation
	 */
	signal?: AbortSignal;

	/**
	 * Optional explicit relay hints to include in routing
	 * These will be merged with router-selected relays
	 */
	relayHints?: string[];

	/**
	 * Optional callback for each event received
	 */
	onEvent?: (event: TrustedEvent, url: string) => void;

	/**
	 * Optional callback when relay closes connection
	 */
	onEose?: (url: string) => void;

	/**
	 * Optional callback when all relays have responded
	 */
	onClose?: () => void;
};

/**
 * Load events using Welshman's outbox model for intelligent relay selection.
 *
 * This automatically:
 * - Queries authors' write relays for their content (NIP-65)
 * - Uses indexer relays for profiles/relay lists
 * - Uses search relays for search queries
 * - Falls back to default relays when needed
 *
 * @example
 * ```typescript
 * // Fetch notes from specific authors - queries their write relays
 * const events = await loadWithRouter({
 *   filters: [{ kinds: [1], authors: ['pubkey1', 'pubkey2'], limit: 50 }]
 * });
 *
 * // Fetch profile - uses indexer relays
 * const profiles = await loadWithRouter({
 *   filters: [{ kinds: [0], authors: ['pubkey'], limit: 1 }]
 * });
 * ```
 */
export async function loadWithRouter(options: LoadWithRouterOptions): Promise<TrustedEvent[]> {
	const { filters, signal, relayHints = [], onEvent, onEose, onClose } = options;

	// Use getFilterSelections to automatically determine optimal relays for each filter
	let selections = getFilterSelections(filters);

	// If we have explicit relay hints, merge them with selections
	if (relayHints.length > 0 && selections.length > 0) {
		selections[0].relays = [...new Set([...relayHints, ...selections[0].relays])];
	}

	// Check if all selections have empty relay arrays
	const hasNoRelays = selections.length === 0 || selections.every(s => s.relays.length === 0);

	// If no selections or all selections have empty relays, use fallback
	if (hasNoRelays) {
		let fallbackRelays: string[] = [];

		if (relayHints.length > 0) {
			fallbackRelays = relayHints;
		} else {
			// Try to get default relays from router context
			const routerRelays = routerContext.getDefaultRelays?.() || [];

			// If router isn't initialized yet (returns empty array), use preset relays
			fallbackRelays = routerRelays.length > 0
				? routerRelays
				: presetRelays.map(r => r.url);
		}

		return load({
			relays: fallbackRelays,
			filters,
			signal,
			onEvent,
			onEose,
			onClose
		});
	}

	// Execute all relay+filter combinations in parallel
	const allEvents = await Promise.all(
		selections.map(({ relays, filters: selectedFilters }) =>
			load({
				relays,
				filters: selectedFilters,
				signal,
				onEvent,
				onEose,
				onClose
			})
		)
	);

	// Flatten and deduplicate results
	const eventMap = new Map<string, TrustedEvent>();
	for (const events of allEvents) {
		for (const event of events) {
			if (!eventMap.has(event.id)) {
				eventMap.set(event.id, event);
			}
		}
	}

	return Array.from(eventMap.values());
}

export type RequestWithRouterOptions = {
	/**
	 * Filters to query
	 */
	filters: Filter[];

	/**
	 * Optional signal for request cancellation
	 */
	signal?: AbortSignal;

	/**
	 * Optional explicit relay hints
	 */
	relayHints?: string[];

	/**
	 * Optional tracker for deduplication
	 */
	tracker?: Tracker;

	/**
	 * Callback for each event received (streaming)
	 */
	onEvent: (event: TrustedEvent, url: string) => void;

	/**
	 * Optional callback when relay sends EOSE
	 */
	onEose?: (url: string) => void;

	/**
	 * Optional callback when relay disconnects
	 */
	onDisconnect?: (url: string) => void;

	/**
	 * Optional callback when all relays have closed
	 */
	onClose?: () => void;

	/**
	 * Auto-close subscription after EOSE
	 */
	autoClose?: boolean;
};

/**
 * Stream events using Welshman's outbox model with router-based relay selection.
 *
 * Similar to loadWithRouter but provides streaming callbacks for real-time updates.
 *
 * @example
 * ```typescript
 * await requestWithRouter({
 *   filters: [{ kinds: [1], authors: ['pubkey'], limit: 50 }],
 *   onEvent: (event, url) => {
 *     console.log('Received event from', url, event);
 *   },
 *   autoClose: true
 * });
 * ```
 */
export async function requestWithRouter(
	options: RequestWithRouterOptions
): Promise<TrustedEvent[]> {
	const {
		filters,
		signal,
		relayHints = [],
		tracker,
		onEvent,
		onEose,
		onDisconnect,
		onClose,
		autoClose
	} = options;

	// Use getFilterSelections for intelligent routing
	let selections = getFilterSelections(filters);

	// Merge relay hints if provided
	if (relayHints.length > 0 && selections.length > 0) {
		selections[0].relays = [...new Set([...relayHints, ...selections[0].relays])];
	}

	// Check if all selections have empty relay arrays
	const hasNoRelays = selections.length === 0 || selections.every(s => s.relays.length === 0);

	// Fallback to relay hints or router's default relays if no usable relays
	if (hasNoRelays) {
		let fallbackRelays: string[] = [];

		if (relayHints.length > 0) {
			fallbackRelays = relayHints;
		} else {
			// Try to get default relays from router context
			const routerRelays = routerContext.getDefaultRelays?.() || [];

			// If router isn't initialized yet (returns empty array), use preset relays
			fallbackRelays = routerRelays.length > 0
				? routerRelays
				: presetRelays.map(r => r.url);
		}

		return request({
			relays: fallbackRelays,
			filters,
			signal,
			tracker,
			onEvent,
			onEose,
			onDisconnect,
			onClose,
			autoClose
		});
	}

	// Execute all relay+filter combinations
	const allEvents = await Promise.all(
		selections.map(({ relays, filters: selectedFilters }) =>
			request({
				relays,
				filters: selectedFilters,
				signal,
				tracker,
				onEvent,
				onEose,
				onDisconnect,
				onClose,
				autoClose
			})
		)
	);

	// Flatten results
	return allEvents.flat();
}
