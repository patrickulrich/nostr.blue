import { ReactNode } from 'react';
import { useNWCInternal as useNWCHook } from '@/hooks/useNWC';
import { NWCContext } from '@/hooks/useNWCContext';

/**
 * Provider component for Nostr Wallet Connect (NWC) context.
 * Makes NWC connection state and payment functions available to child components.
 * @param props - Provider props containing children elements
 */
export function NWCProvider({ children }: { children: ReactNode }) {
  const nwc = useNWCHook();
  return <NWCContext.Provider value={nwc}>{children}</NWCContext.Provider>;
}