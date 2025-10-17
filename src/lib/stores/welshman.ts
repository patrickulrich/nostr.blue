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
import { RelayMode } from '@welshman/util';
import { Pool } from '@welshman/net';

/**
 * Get relay quality score (0-1)
 * Higher scores indicate better relay performance
 */
function getRelayQuality(url: string): number {
	// TODO: Track relay performance and return actual quality scores
	// For now, return a default quality
	return 0.5;
}

/**
 * Get relays for a given pubkey
 * This will query NIP-65 relay lists in the future
 */
function getPubkeyRelays(pk: string, mode?: RelayMode): string[] {
	// TODO: Query kind 10002 (NIP-65) relay lists for this pubkey
	// For now, return default relays
	const config = get(appConfig);
	return [config.relayUrl, ...presetRelays.map(r => r.url)].slice(0, 5);
}

/**
 * Welshman Router store
 *
 * Manages Welshman Router configuration and connection pool
 */
function createWelshmanStore() {
	const { subscribe, set, update } = writable<boolean>(false);

	return {
		subscribe,
		init: () => {
			if (!browser) return;

			// Configure Welshman Router
			const config = get(appConfig);
			const defaultRelays = [config.relayUrl, ...presetRelays.map(r => r.url)].slice(0, 5);

			routerContext.getUserPubkey = () => get(pubkey) || '';
			routerContext.getDefaultRelays = () => defaultRelays;
			routerContext.getPubkeyRelays = getPubkeyRelays;
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
