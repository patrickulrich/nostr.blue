import { useQuery } from '@tanstack/react-query';
import { nip19 } from 'nostr-tools';

interface NostrEvent {
  id: string;
  pubkey: string;
  created_at: number;
  kind: number;
  tags: string[][];
  content: string;
  sig: string;
}

interface SuggestedProfile {
  pubkey: string;
  relays: string[];
  profile?: NostrEvent;
}

interface ParsedProfile {
  pubkey: string;
  profile?: {
    name?: string;
    display_name?: string;
    picture?: string;
    nip05?: string;
    about?: string;
  };
}

interface NostrBandSuggestedResponse {
  profiles: SuggestedProfile[];
}

/**
 * Hook to fetch suggested profiles from nostr.band API
 * Returns profiles recommended for following based on the user's pubkey
 */
export function useSuggestedProfiles(pubkey: string | undefined, limit: number = 5) {
  return useQuery<ParsedProfile[]>({
    queryKey: ['suggested-profiles', pubkey, limit],
    queryFn: async ({ signal }) => {
      if (!pubkey) {
        return [];
      }

      try {
        // Convert hex pubkey to npub format for the API
        const npub = nip19.npubEncode(pubkey);
        const url = `https://api.nostr.band/v0/suggested/profiles/${npub}`;

        // Create a timeout signal (10 seconds)
        const timeoutSignal = AbortSignal.timeout(10000);
        const combinedSignal = signal ? AbortSignal.any([signal, timeoutSignal]) : timeoutSignal;

        const response = await fetch(url, {
          signal: combinedSignal,
          mode: 'cors',
          headers: {
            'Accept': 'application/json',
          }
        });

        if (!response.ok) {
          throw new Error(`Failed to fetch suggested profiles: ${response.status}`);
        }

        const data: NostrBandSuggestedResponse = await response.json();

        // Return limited number of profiles, parsing the profile content
        const profiles = (data.profiles || []).slice(0, limit);

        return profiles.map(item => {
          let parsedProfile;
          if (item.profile?.content) {
            try {
              parsedProfile = JSON.parse(item.profile.content);
              // Fix HTTP image URLs to HTTPS
              if (parsedProfile.picture) {
                parsedProfile.picture = parsedProfile.picture.replace(/^http:/, 'https:');
              }
              if (parsedProfile.banner) {
                parsedProfile.banner = parsedProfile.banner.replace(/^http:/, 'https:');
              }
            } catch {
              // Silently ignore JSON parse errors
            }
          }

          return {
            pubkey: item.pubkey,
            profile: parsedProfile,
          };
        });
      } catch (error) {
        // Silently return empty array for timeout and abort errors
        if (error instanceof Error && (error.name === 'TimeoutError' || error.name === 'AbortError')) {
          return [];
        }
        // Log other errors only in development
        if (import.meta.env.DEV) {
          console.error('[useSuggestedProfiles] Failed to fetch suggested profiles:', error);
        }
        return [];
      }
    },
    enabled: !!pubkey,
    staleTime: 10 * 60 * 1000, // Cache for 10 minutes
    gcTime: 30 * 60 * 1000, // Keep in cache for 30 minutes
    retry: 1,
  });
}
