import { NSchema as n } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery } from '@tanstack/react-query';

/**
 * User search result containing profile metadata.
 */
export interface UserSearchResult {
  pubkey: string;
  name?: string;
  displayName?: string;
  picture?: string;
  nip05?: string;
}

/**
 * Hook to search for users by name or NIP-05
 */
export function useSearchUsers(query: string) {
  const { nostr } = useNostr();
  const q = query.trim();

  return useQuery<UserSearchResult[]>({
    queryKey: ['search-users', q],
    queryFn: async ({ signal }) => {
      if (q.length < 2) return [];

      // Search for kind 0 events (user metadata)
      // We'll search by fetching recent profiles and filter client-side
      // In a production app, you'd use a relay that supports NIP-50 text search
      const events = await nostr.query(
        [{ kinds: [0], limit: 100 }],
        { signal: AbortSignal.any([signal, AbortSignal.timeout(2000)]) }
      );

      const results: UserSearchResult[] = [];
      const seenPubkeys = new Set<string>();

      for (const event of events) {
        if (seenPubkeys.has(event.pubkey)) continue;
        seenPubkeys.add(event.pubkey);

        try {
          const metadata = n.json().pipe(n.metadata()).parse(event.content);
          const name = metadata.name?.toLowerCase() || '';
          const displayName = metadata.display_name?.toLowerCase() || '';
          const nip05 = metadata.nip05?.toLowerCase() || '';
          const queryLower = q.toLowerCase();

          // Check if query matches any field
          if (
            name.includes(queryLower) ||
            displayName.includes(queryLower) ||
            nip05.includes(queryLower)
          ) {
            results.push({
              pubkey: event.pubkey,
              name: metadata.name,
              displayName: metadata.display_name,
              picture: metadata.picture,
              nip05: metadata.nip05,
            });
          }
        } catch {
          // Skip invalid metadata
        }

        // Limit results
        if (results.length >= 10) break;
      }

      return results;
    },
    enabled: q.length >= 2,
    staleTime: 30000,
  });
}
