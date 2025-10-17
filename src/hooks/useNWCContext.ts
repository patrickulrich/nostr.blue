import { useContext } from 'react';
import { createContext } from 'react';
import { useNWCInternal } from '@/hooks/useNWC';

type NWCContextType = ReturnType<typeof useNWCInternal>;

/** React context for Nostr Wallet Connect state */
export const NWCContext = createContext<NWCContextType | null>(null);

/**
 * Hook to access Nostr Wallet Connect context.
 * Must be used within a NWCProvider.
 *
 * @throws Error if used outside of NWCProvider
 * @returns NWC context with connection management and payment functions
 */
export function useNWC(): NWCContextType {
  const context = useContext(NWCContext);
  if (!context) {
    throw new Error('useNWC must be used within a NWCProvider');
  }
  return context;
}