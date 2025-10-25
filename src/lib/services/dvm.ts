/**
 * Custom DVM request implementation with persistent subscriptions
 *
 * This bypasses welshman's makeDVMFeed which uses autoClose: true,
 * preventing async DVM responses from being received.
 */

import { publish, request } from '@welshman/net';
import { signer as welshmanSigner, currentPubkey } from '$lib/stores/auth';
import { makeEvent } from '@welshman/util';
import type { TrustedEvent, Filter } from '@welshman/util';
import { get } from 'svelte/store';

// DVM-specific relays
const DVM_RELAYS = [
	'wss://relay.damus.io',
	'wss://relay.primal.net',
	'wss://relay.nostr.band',
	'wss://nos.lol',
	'wss://offchain.pub'
];

export interface DVMRequestOptions {
	dvmPubkey: string;
	requestKind: number; // e.g., 5300 for content discovery
	params?: Record<string, string>;
	onResponse?: (event: TrustedEvent) => void;
	onEvent?: (event: TrustedEvent) => void;
	signal?: AbortSignal;
}

/**
 * Request content from a DVM and keep subscription open for async responses
 *
 * Unlike welshman's requestDVM which uses autoClose: true,
 * this keeps subscriptions open indefinitely to receive async DVM responses.
 */
export async function requestDVM(options: DVMRequestOptions): Promise<() => void> {
	const { dvmPubkey, requestKind, params = {}, onResponse, onEvent, signal } = options;

	const signer = get(welshmanSigner);
	const userPubkey = get(currentPubkey);

	if (!signer || !userPubkey) {
		throw new Error('User must be logged in to request from DVMs');
	}

	// Build DVM request event tags
	const tags: string[][] = [
		['p', dvmPubkey], // DVM to handle this request
		['param', 'user', userPubkey], // User requesting
		['param', 'max_results', '200'], // Max results to return
	];

	// Add custom parameters
	for (const [key, value] of Object.entries(params)) {
		tags.push(['param', key, value]);
	}

	// Sign and create the DVM request event
	const requestEvent = await signer.sign(makeEvent(requestKind, { tags, content: '' }));

	// Publish the request to DVM relays
	const publishPromise = publish({
		event: requestEvent,
		relays: DVM_RELAYS
	});

	// Subscribe to DVM responses (kind + 1000, tagged with request ID)
	// IMPORTANT: autoClose: false - subscription stays open for async responses
	const responseKind = requestKind + 1000; // e.g., 6300 for kind 5300
	const filters: Filter[] = [
		{
			kinds: [responseKind],
			'#e': [requestEvent.id], // Must reference our request
			since: Math.floor(Date.now() / 1000) - 60 // Last 60 seconds
		}
	];

	// Create abort controller for cleanup
	const abortController = new AbortController();
	const requestSignal = signal ? AbortSignal.any([signal, abortController.signal]) : abortController.signal;

	// Subscribe WITHOUT autoClose - keeps subscription open
	const responsePromise = request({
		filters,
		relays: DVM_RELAYS,
		signal: requestSignal,
		autoClose: false, // CRITICAL: Keep subscription open for async DVM responses
		onEvent: async (event: TrustedEvent) => {
			// Callback for the raw response
			onResponse?.(event);

			// Parse response content to extract event references
			try {
				const tags = JSON.parse(event.content);

				// Extract event IDs from 'e' tags
				const eventIds = tags
					.filter((tag: string[]) => tag[0] === 'e')
					.map((tag: string[]) => tag[1]);

				if (eventIds.length > 0) {
					// Fetch the actual events
					const eventFilters: Filter[] = [{ ids: eventIds }];
					await request({
						filters: eventFilters,
						relays: DVM_RELAYS,
						signal: requestSignal,
						autoClose: true, // Can close after fetching these events
						onEvent: (event: TrustedEvent) => {
							onEvent?.(event);
						}
					});
				}
			} catch (error) {
				console.error('[DVM] Failed to parse response:', error);
			}
		}
	});

	// Wait for publish to complete
	await publishPromise;

	// Return cleanup function
	return () => {
		abortController.abort();
	};
}
