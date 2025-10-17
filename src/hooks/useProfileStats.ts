import { useQuery } from '@tanstack/react-query';

interface ProfileStats {
  pubkey: string;
  followers_pubkey_count?: number;
  pub_following_pubkey_count?: number;
  pub_note_count?: number;
  pub_reply_count?: number;
  pub_repost_count?: number;
  pub_reaction_count?: number;
  pub_zap_sent_count?: number;
  pub_zap_sent_total_msats?: number;
  followers_count?: number;
  following_count?: number;
}

interface NostrBandStatsResponse {
  stats: {
    [pubkey: string]: ProfileStats;
  };
}

/**
 * Hook to fetch profile statistics from nostr.band API
 * Returns follower count, following count, and other engagement metrics
 */
export function useProfileStats(pubkey: string | undefined) {
  return useQuery<ProfileStats | null>({
    queryKey: ['profile-stats', pubkey],
    queryFn: async ({ signal }) => {
      if (!pubkey) {
        return null;
      }

      try {
        const response = await fetch(
          `https://api.nostr.band/v0/stats/profile/${pubkey}`,
          { signal }
        );

        if (!response.ok) {
          throw new Error('Failed to fetch profile stats');
        }

        const data: NostrBandStatsResponse = await response.json();

        // The response has stats keyed by pubkey
        return data.stats[pubkey] || null;
      } catch (error) {
        // Don't log AbortError as it's expected when queries are cancelled
        if (error instanceof Error && error.name === 'AbortError') {
          return null;
        }
        console.error('Failed to fetch profile stats:', error);
        return null;
      }
    },
    enabled: !!pubkey,
    staleTime: 5 * 60 * 1000, // Cache for 5 minutes
    gcTime: 10 * 60 * 1000, // Keep in cache for 10 minutes
    retry: 1,
  });
}
