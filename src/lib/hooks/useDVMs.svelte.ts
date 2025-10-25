import { repository } from '@welshman/app';
import { derived } from 'svelte/store';
import { deriveEvents } from '@welshman/store';
import type { TrustedEvent } from '@welshman/util';
import { fromPairs, groupBy } from '@welshman/lib';
import { load } from '@welshman/net';
import { presetRelays } from '$lib/stores/appStore';
import { browser } from '$app/environment';

const DVM_ANNOUNCEMENT_KIND = 31990;

// Track if we've loaded DVMs
let dvmsLoaded = false;

export interface DVMService {
	id: string;
	pubkey: string;
	name?: string;
	about?: string;
	picture?: string;
	supportedKinds: number[];
	tags: string[];
	handlers: {
		platform: string;
		url: string;
		entityType?: string;
	}[];
	event: TrustedEvent;
}

/**
 * Parse DVM events into DVMService objects
 */
function parseDVMEvents(events: TrustedEvent[]): DVMService[] {
	return events.map((event) => {
		// Parse metadata from content field if present
		let metadata: { name?: string; about?: string; picture?: string } = {};
		if (event.content) {
			try {
				metadata = JSON.parse(event.content);
			} catch {
				// Ignore parsing errors
			}
		}

		// Extract d tag (identifier)
		const dTag = fromPairs(event.tags).d || '';

		// For parameterized replaceable events, unique ID is pubkey + d tag
		const id = `${event.pubkey}:${dTag}`;

		// Extract supported kinds
		const supportedKinds = event.tags
			.filter((tag) => tag[0] === 'k')
			.map((tag) => parseInt(tag[1], 10))
			.filter((k) => !isNaN(k));

		// Extract topic tags
		const topicTags = event.tags.filter((tag) => tag[0] === 't').map((tag) => tag[1]);

		// Extract handlers (web, ios, android, etc.)
		const handlers = event.tags
			.filter((tag) => ['web', 'ios', 'android', 'desktop'].includes(tag[0]))
			.map((tag) => ({
				platform: tag[0],
				url: tag[1],
				entityType: tag[2] // e.g., 'nevent', 'nprofile', etc.
			}));

		return {
			id,
			pubkey: event.pubkey,
			name: metadata.name,
			about: metadata.about,
			picture: metadata.picture,
			supportedKinds,
			tags: topicTags,
			handlers,
			event
		};
	});
}

/**
 * Derived store for DVM handler events from welshman repository
 * This is reactive and will automatically update when new events arrive
 *
 * Kind 31990 is parameterized replaceable, so we deduplicate by pubkey+dtag
 */
export const dvmHandlerEvents = derived(
	deriveEvents(repository, { filters: [{ kinds: [DVM_ANNOUNCEMENT_KIND] }] }),
	($events) => {
		// Deduplicate by pubkey + d tag, keeping only most recent
		const seen = new Map<string, TrustedEvent>();

		for (const event of $events) {
			const dTag = fromPairs(event.tags).d || '';
			const key = `${event.pubkey}:${dTag}`;

			const existing = seen.get(key);
			if (!existing || event.created_at > existing.created_at) {
				seen.set(key, event);
			}
		}

		return Array.from(seen.values()).sort((a, b) => b.created_at - a.created_at);
	}
);

/**
 * Derived store for parsed DVM services
 */
export const dvmServices = derived(dvmHandlerEvents, ($events) => {
	const services = parseDVMEvents($events);
	return services;
});

/**
 * Derived store for DVMs indexed by kind
 */
export const dvmsByKind = derived(dvmServices, ($services) => {
	return groupBy((dvm: DVMService) => {
		// Return all supported kinds for this DVM
		return dvm.supportedKinds.length > 0 ? dvm.supportedKinds : [0];
	}, $services);
});

/**
 * Load DVM announcements from relays into the repository
 */
async function loadDVMsIntoRepository() {
	if (!browser || dvmsLoaded) return;

	dvmsLoaded = true;

	try {
		// Use preset relays directly - router might not be initialized yet
		const relays = presetRelays.map((r) => r.url);

		// Load events into repository
		const events = await load({
			relays,
			filters: [{ kinds: [DVM_ANNOUNCEMENT_KIND], limit: 100 }]
		});

		// Manually publish to repository to ensure they're there
		events.forEach((event) => repository.publish(event));

		console.log('[useDVMs] Loaded', events.length, 'DVM announcements');
	} catch (error) {
		console.error('[useDVMs] Failed to load DVM announcements:', error);
		dvmsLoaded = false; // Allow retry
	}
}

/**
 * Hook to access DVM services
 * Uses welshman's repository for native reactivity
 */
export function useDVMs() {
	// Load DVMs on first call
	loadDVMsIntoRepository();

	// Return the stores - they're already reactive
	return {
		dvmHandlerEvents,
		dvmServices,
		dvmsByKind
	};
}
