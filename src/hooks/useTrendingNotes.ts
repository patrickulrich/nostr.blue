import { useQuery } from '@tanstack/react-query';
import type { NostrEvent } from '@nostrify/nostrify';

export interface TrendingNote {
  event: NostrEvent;
  author: {
    id?: string;
    pubkey: string;
    content: string; // JSON string containing profile data
  };
  profile?: {
    name?: string;
    display_name?: string;
    picture?: string;
    nip05?: string;
    about?: string;
    banner?: string;
    website?: string;
    lud16?: string;
  };
  stats?: {
    replies?: number;
    reactions?: number;
    reposts?: number;
    zaps?: number;
  };
}

interface NostrBandApiNote {
  event: NostrEvent;
  author: {
    id?: string;
    pubkey: string;
    content: string;
  };
  stats?: {
    replies_count?: number;
    reactions_count?: number;
    reposts_count?: number;
    zaps_msats?: number;
  };
}

interface NostrBandResponse {
  notes: NostrBandApiNote[];
}

/**
 * Hook to fetch trending notes from nostr.band API
 * Returns the top trending posts in the last 24 hours
 */
export function useTrendingNotes(limit?: number) {
  return useQuery<TrendingNote[]>({
    queryKey: ['trending-notes', limit],
    queryFn: async ({ signal }) => {
      try {
        const response = await fetch('https://api.nostr.band/v0/trending/notes', {
          signal,
        });

        if (!response.ok) {
          throw new Error('Failed to fetch trending notes');
        }

        const data: NostrBandResponse = await response.json();

        // Check if data.notes exists
        if (!data.notes || !Array.isArray(data.notes)) {
          console.warn('Invalid response format from nostr.band:', data);
          return [];
        }

        // Parse and transform the notes
        const parsedNotes: TrendingNote[] = data.notes.map((note) => {
          // Parse the author's content field to get profile data
          let profile;
          try {
            profile = JSON.parse(note.author.content);
          } catch (e) {
            console.warn('Failed to parse author profile:', e);
            profile = {};
          }

          return {
            event: note.event,
            author: note.author,
            profile: profile,
            stats: {
              replies: note.stats?.replies_count,
              reactions: note.stats?.reactions_count,
              reposts: note.stats?.reposts_count,
              zaps: note.stats?.zaps_msats ? Math.floor(note.stats.zaps_msats / 1000) : undefined,
            },
          };
        });

        // Return limited or all trending notes
        return limit ? parsedNotes.slice(0, limit) : parsedNotes;
      } catch (error) {
        // Don't log AbortError as it's expected when queries are cancelled
        if (error instanceof Error && error.name === 'AbortError') {
          return [];
        }
        console.error('Failed to fetch trending notes:', error);
        return [];
      }
    },
    staleTime: 15 * 60 * 1000, // Cache for 15 minutes
    gcTime: 30 * 60 * 1000, // Keep in cache for 30 minutes
    retry: 1,
  });
}
