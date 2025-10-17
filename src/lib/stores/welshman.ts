/**
 * Welshman integration store
 *
 * This store manages the Welshman Router and connection setup for Nostr.
 * Welshman is a modern Nostr toolkit extracted from Coracle.
 */

import { writable, derived, get } from 'svelte/store';
import { browser } from '$app/environment';
import { appConfig, presetRelays } from './appStore';

// TODO: Import Welshman packages once they're set up
// import { Router } from '@welshman/router';
// import { connect } from '@welshman/net';

/**
 * Welshman Router store
 *
 * This will be initialized once Welshman is properly integrated.
 * For now, this is a placeholder structure.
 */
function createWelshmanStore() {
  const { subscribe, set, update } = writable<any>(null);

  return {
    subscribe,
    init: () => {
      if (!browser) return;

      // TODO: Initialize Welshman Router
      // const router = new Router({
      //   getRelays: () => {
      //     const config = get(appConfig);
      //     return [config.relayUrl, ...presetRelays.map(r => r.url)];
      //   }
      // });

      // set(router);

      console.log('Welshman Router initialization placeholder');
    },
    cleanup: () => {
      // TODO: Cleanup connections
      set(null);
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
