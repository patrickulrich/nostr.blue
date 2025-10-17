import { nwcStore, type NWCConnection } from './nwc.svelte';
import type { WebLNProvider } from '@webbtc/webln-types';

export interface WalletStatus {
  hasNWC: boolean;
  webln: WebLNProvider | null;
  activeNWC: NWCConnection | null;
  preferredMethod: 'nwc' | 'webln' | 'manual';
}

/**
 * Wallet status utilities for Svelte 5
 * Provides wallet connection status and preferred payment method
 */
export function useWallet(): WalletStatus {
  // Get active NWC connection
  const activeNWC = nwcStore.getActiveConnection();

  // Access WebLN from browser global scope
  const webln = typeof globalThis !== 'undefined'
    ? (globalThis as { webln?: WebLNProvider }).webln || null
    : null;

  // Check if we have any connected NWC wallets
  const hasNWC = nwcStore.connections.length > 0 &&
    nwcStore.connections.some(c => c.isConnected);

  // Determine preferred payment method
  const preferredMethod: WalletStatus['preferredMethod'] = activeNWC
    ? 'nwc'
    : webln
    ? 'webln'
    : 'manual';

  return {
    hasNWC,
    webln,
    activeNWC,
    preferredMethod,
  };
}

// For reactive contexts, use this to get wallet status
export function getWalletStatus(): WalletStatus {
  return useWallet();
}
