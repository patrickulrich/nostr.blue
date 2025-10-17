/**
 * Welshman integration store
 *
 * This store manages the Welshman Router and connection setup for Nostr.
 * Welshman is a modern Nostr toolkit extracted from Coracle.
 */

import { writable, derived, get } from 'svelte/store';
import { browser } from '$app/environment';
import { appConfig, presetRelays } from './appStore';
import { routerContext } from '@welshman/router';
import { pubkey } from '@welshman/app';
import { RelayMode, RELAYS, readList, getRelaysFromList, asDecryptedEvent } from '@welshman/util';
import { Pool, load } from '@welshman/net';

// Relay performance tracking
interface RelayStats {
	url: string;
	connected: boolean;
	latency: number; // Average latency in ms
	errors: number;
	successes: number;
	lastSeen: number; // Timestamp
}

const relayStats = new Map<string, RelayStats>();

/**
 * Update relay statistics
 */
function updateRelayStats(url: string, success: boolean, latency?: number) {
	const stats = relayStats.get(url) || {
		url,
		connected: false,
		latency: 0,
		errors: 0,
		successes: 0,
		lastSeen: 0
	};

	if (success) {
		stats.successes++;
		if (latency !== undefined) {
			// Calculate rolling average latency
			stats.latency = stats.latency === 0
				? latency
				: (stats.latency * 0.7 + latency * 0.3);
		}
	} else {
		stats.errors++;
	}

	stats.lastSeen = Date.now();
	relayStats.set(url, stats);
}

/**
 * Get relay quality score (0-1)
 * Higher scores indicate better relay performance
 *
 * Quality is calculated based on:
 * - Success rate (errors vs successes)
 * - Latency (lower is better)
 * - Recency (recently seen relays score higher)
 */
function getRelayQuality(url: string): number {
	const stats = relayStats.get(url);

	// Default quality for unknown relays
	if (!stats || (stats.errors + stats.successes) === 0) {
		return 0.5;
	}

	// Success rate score (0-0.5)
	const totalRequests = stats.errors + stats.successes;
	const successRate = stats.successes / totalRequests;
	const successScore = successRate * 0.5;

	// Latency score (0-0.3)
	// Target: <500ms = 0.3, >2000ms = 0
	const latencyScore = stats.latency === 0
		? 0.15
		: Math.max(0, 0.3 * (1 - stats.latency / 2000));

	// Recency score (0-0.2)
	// Active in last 5 minutes = 0.2, older = 0
	const age = Date.now() - stats.lastSeen;
	const recencyScore = Math.max(0, 0.2 * (1 - age / (5 * 60 * 1000)));

	return Math.min(1, successScore + latencyScore + recencyScore);
}

// Cache for NIP-65 relay lists
const relayListCache = new Map<string, { relays: string[]; timestamp: number }>();
const CACHE_DURATION = 5 * 60 * 1000; // 5 minutes

/**
 * Get relays for a given pubkey from their NIP-65 relay list (kind 10002)
 *
 * Uses Welshman utilities to properly parse relay lists with read/write markers
 *
 * @param pk - The pubkey to query relay lists for
 * @param mode - Filter relays by read/write mode (optional)
 * @returns Array of relay URLs
 */
async function getPubkeyRelays(pk: string, mode?: RelayMode): Promise<string[]> {
	// Check cache first
	const cacheKey = `${pk}:${mode || 'all'}`;
	const cached = relayListCache.get(cacheKey);
	if (cached && Date.now() - cached.timestamp < CACHE_DURATION) {
		return cached.relays;
	}

	try {
		// Query kind 10002 (NIP-65 relay list) for this pubkey
		const events = await load({
			relays: [], // Use default relays from router
			filters: [
				{
					kinds: [RELAYS], // kind 10002
					authors: [pk],
					limit: 1
				}
			]
		});

		if (events.length === 0) {
			// No relay list found, use defaults
			const config = get(appConfig);
			const defaultRelays = [config.relayUrl, ...presetRelays.map(r => r.url)].slice(0, 5);
			relayListCache.set(cacheKey, { relays: defaultRelays, timestamp: Date.now() });
			return defaultRelays;
		}

		// Parse relay list using Welshman utilities
		const relayEvent = events[0];
		const list = readList(asDecryptedEvent(relayEvent));
		const relayUrls = getRelaysFromList(list, mode);

		// Use defaults if no relays found
		if (relayUrls.length === 0) {
			const config = get(appConfig);
			const defaultRelays = [config.relayUrl, ...presetRelays.map(r => r.url)].slice(0, 5);
			relayListCache.set(cacheKey, { relays: defaultRelays, timestamp: Date.now() });
			return defaultRelays;
		}

		// Cache the results
		relayListCache.set(cacheKey, { relays: relayUrls, timestamp: Date.now() });
		return relayUrls;
	} catch (error) {
		console.error('Failed to fetch NIP-65 relay list:', error);

		// Fall back to defaults
		const config = get(appConfig);
		const defaultRelays = [config.relayUrl, ...presetRelays.map(r => r.url)].slice(0, 5);
		return defaultRelays;
	}
}

/**
 * Wrap getPubkeyRelays for synchronous router context
 * This caches the promise and returns cached relays or defaults
 */
function getPubkeyRelaysSync(pk: string, mode?: RelayMode): string[] {
	const cacheKey = `${pk}:${mode || 'all'}`;
	const cached = relayListCache.get(cacheKey);

	if (cached && Date.now() - cached.timestamp < CACHE_DURATION) {
		return cached.relays;
	}

	// Start async fetch in background
	getPubkeyRelays(pk, mode).catch(console.error);

	// Return defaults immediately
	const config = get(appConfig);
	return [config.relayUrl, ...presetRelays.map(r => r.url)].slice(0, 5);
}

/**
 * Welshman Router store
 *
 * Manages Welshman Router configuration and connection pool
 */
function createWelshmanStore() {
	const { subscribe, set } = writable<boolean>(false);

	return {
		subscribe,
		init: () => {
			if (!browser) return;

			// Configure Welshman Router
			const config = get(appConfig);
			const defaultRelays = [config.relayUrl, ...presetRelays.map(r => r.url)].slice(0, 5);

			routerContext.getUserPubkey = () => get(pubkey) || '';
			routerContext.getDefaultRelays = () => defaultRelays;
			routerContext.getPubkeyRelays = getPubkeyRelaysSync;
			routerContext.getRelayQuality = getRelayQuality;
			routerContext.getIndexerRelays = () => defaultRelays;
			routerContext.getSearchRelays = () => defaultRelays;

			// Mark as initialized
			set(true);

			console.log('Welshman Router initialized with relays:', defaultRelays);
		},
		cleanup: () => {
			if (!browser) return;

			// Close all relay connections
			const pool = Pool.get();
			pool.clear();

			set(false);
			console.log('Welshman Router cleaned up');
		}
	};
}

export const welshmanRouter = createWelshmanStore();

/**
 * Current relay URL from config
 */
export const currentRelayUrl = derived(
  appConfig,
  ($appConfig) => $appConfig.relayUrl
);

/**
 * All relay URLs (current + presets)
 */
export const allRelayUrls = derived(
  appConfig,
  ($appConfig) => {
    const urls = new Set<string>([
      $appConfig.relayUrl,
      ...presetRelays.map(r => r.url)
    ]);
    return Array.from(urls).slice(0, 5); // Cap at 5 relays
  }
);

/**
 * Get relay statistics for debugging and monitoring
 */
export function getRelayStats() {
	return Array.from(relayStats.entries()).map(([url, stats]) => ({
		...stats,
		quality: getRelayQuality(url)
	}));
}

/**
 * Export relay management functions
 */
export { getPubkeyRelays, updateRelayStats };
