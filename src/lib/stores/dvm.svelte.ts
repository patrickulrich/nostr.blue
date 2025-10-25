/**
 * DVM (Data Vending Machine) Store
 *
 * Provides functions to interact with Nostr DVMs for AI-curated content.
 * Uses Welshman Router for intelligent relay selection.
 */

import { loadWithRouter } from '$lib/services/outbox';
import type { TrustedEvent } from '@welshman/util';

// Popular feed DVM pubkey
export const POPULAR_DVM_PUBKEY = '96945c769ef9e91be05570fef1003633f5bb9d072ba2453781b5140013ab35b3';

/**
 * Query DVM feed results from a specific DVM
 *
 * @param dvmPubkey - The DVM's pubkey
 * @param resultKind - The kind of result events to query (e.g., 6300 for popular feed)
 * @param limit - Maximum number of results to fetch
 * @returns Array of DVM result events
 */
export async function queryDVMFeed(
	dvmPubkey: string,
	resultKind: number,
	limit: number = 10
): Promise<TrustedEvent[]> {
	try {
		const events = await loadWithRouter({
			filters: [
				{
					kinds: [resultKind],
					authors: [dvmPubkey],
					limit
				}
			]
		});

		return events.sort((a, b) => b.created_at - a.created_at);
	} catch (error) {
		console.error('Failed to fetch DVM feed:', error);
		return [];
	}
}

/**
 * Parse event IDs from DVM result content
 *
 * DVM results can return:
 * - JSON array of event IDs (strings)
 * - JSON array of ["e", eventId] tags
 * - JSON array of event objects
 * - Plain text with event IDs (one per line)
 */
export function parseDVMEventIds(content: string): string[] {
	const ids: string[] = [];

	try {
		const trimmed = content.trim();

		if (trimmed.startsWith('[') || trimmed.startsWith('{')) {
			// Try parsing as JSON
			const parsed = JSON.parse(trimmed);

			if (Array.isArray(parsed)) {
				parsed.forEach((item) => {
					if (Array.isArray(item) && item.length >= 2 && item[0] === 'e') {
						// ["e", eventId] tag format
						const eventId = item[1];
						if (typeof eventId === 'string' && eventId.length === 64) {
							ids.push(eventId);
						}
					} else if (typeof item === 'object' && item.kind !== undefined) {
						// Full event object
						if (typeof item.id === 'string') {
							ids.push(item.id);
						}
					} else if (typeof item === 'string' && item.length === 64) {
						// Plain event ID string
						ids.push(item);
					}
				});
			} else if (typeof parsed === 'object' && parsed.kind !== undefined) {
				// Single event object
				if (typeof parsed.id === 'string') {
					ids.push(parsed.id);
				}
			}
		} else {
			// Plain text format - event IDs one per line
			const lines = trimmed.split('\n').map((l) => l.trim()).filter((l) => l.length === 64);
			ids.push(...lines);
		}
	} catch (e) {
		console.error('Failed to parse DVM result:', e);
	}

	return ids;
}

/**
 * Fetch events by IDs using Welshman Router
 */
export async function fetchEventsByIds(ids: string[]): Promise<TrustedEvent[]> {
	if (ids.length === 0) return [];

	try {
		const events = await loadWithRouter({
			filters: [
				{
					ids
				}
			]
		});

		// Sort by DVM's ranking order (preserve order from ids array)
		const eventMap = new Map(events.map((e) => [e.id, e]));
		return ids.map((id) => eventMap.get(id)).filter((e): e is TrustedEvent => e !== undefined);
	} catch (error) {
		console.error('Failed to fetch events by IDs:', error);
		return [];
	}
}
