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
      console.log('[useSuggestedProfiles] Starting query with pubkey:', pubkey);

      if (!pubkey) {
        console.log('[useSuggestedProfiles] No pubkey, returning empty array');
        return [];
      }

      try {
        // Convert hex pubkey to npub format for the API
        const npub = nip19.npubEncode(pubkey);
        const url = `https://api.nostr.band/v0/suggested/profiles/${npub}`;
        console.log('[useSuggestedProfiles] Using npub:', npub);
        console.log('[useSuggestedProfiles] Fetching from URL:', url);

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

        console.log('[useSuggestedProfiles] Response status:', response.status);

        if (!response.ok) {
          const errorText = await response.text();
          console.error(`[useSuggestedProfiles] API error: ${response.status} ${response.statusText}`, errorText);
          throw new Error(`Failed to fetch suggested profiles: ${response.status}`);
        }

        const data: NostrBandSuggestedResponse = await response.json();
        console.log('[useSuggestedProfiles] Response data:', data);
        console.log('[useSuggestedProfiles] Number of profiles:', data.profiles?.length);

        // Return limited number of profiles, parsing the profile content
        const profiles = (data.profiles || []).slice(0, limit);
        console.log('[useSuggestedProfiles] Processing profiles:', profiles);

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
            } catch (e) {
              console.warn('[useSuggestedProfiles] Failed to parse profile content:', e);
            }
          }

          return {
            pubkey: item.pubkey,
            profile: parsedProfile,
          };
        });
      } catch (error) {
        // Log timeout errors
        if (error instanceof Error && error.name === 'TimeoutError') {
          console.error('[useSuggestedProfiles] Request timed out after 10 seconds');
          return [];
        }
        // Don't log AbortError as it's expected when queries are cancelled
        if (error instanceof Error && error.name === 'AbortError') {
          console.log('[useSuggestedProfiles] Request aborted');
          return [];
        }
        console.error('[useSuggestedProfiles] Failed to fetch suggested profiles:', error);
        if (error instanceof Error) {
          console.error('[useSuggestedProfiles] Error name:', error.name);
          console.error('[useSuggestedProfiles] Error message:', error.message);
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
