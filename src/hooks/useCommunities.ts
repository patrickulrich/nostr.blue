import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery } from '@tanstack/react-query';
import { useCurrentUser } from './useCurrentUser';

export interface Community {
  id: string;
  pubkey: string;
  dTag: string; // community identifier
  name?: string;
  description?: string;
  image?: string;
  moderators: string[];
  relays: string[];
  event: NostrEvent;
  aTag: string; // formatted as "34550:pubkey:dTag"
}

export function useCommunities() {
  const { nostr } = useNostr();

  return useQuery<Community[]>({
    queryKey: ['communities'],
    queryFn: async ({ signal }) => {
      try {
        const events = await nostr.query(
          [{ kinds: [34550], limit: 100 }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
        );

        const communities: Community[] = events.map(event => {
          const dTag = event.tags.find(t => t[0] === 'd')?.[1] || '';
          const name = event.tags.find(t => t[0] === 'name')?.[1];
          const description = event.tags.find(t => t[0] === 'description')?.[1];
          const image = event.tags.find(t => t[0] === 'image')?.[1];
          const moderators = event.tags
            .filter(t => t[0] === 'p' && t[3] === 'moderator')
            .map(t => t[1]);
          const relays = event.tags
            .filter(t => t[0] === 'relay')
            .map(t => t[1]);

          return {
            id: event.id,
            pubkey: event.pubkey,
            dTag,
            name: name || dTag,
            description,
            image,
            moderators,
            relays,
            event,
            aTag: `34550:${event.pubkey}:${dTag}`,
          };
        });

        return communities.sort((a, b) =>
          b.event.created_at - a.event.created_at
        );
      } catch (error) {
        console.error('Failed to fetch communities:', error);
        return [];
      }
    },
    staleTime: 60000, // 1 minute
  });
}

export function useCommunity(aTag: string) {
  const { nostr } = useNostr();

  // Parse aTag: "34550:pubkey:dTag"
  const [kind, pubkey, dTag] = aTag.split(':');

  return useQuery<Community | null>({
    queryKey: ['community', aTag],
    queryFn: async ({ signal }) => {
      if (!pubkey || !dTag) return null;

      try {
        const events = await nostr.query(
          [{ kinds: [34550], authors: [pubkey], '#d': [dTag], limit: 1 }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
        );

        const event = events[0];
        if (!event) return null;

        const name = event.tags.find(t => t[0] === 'name')?.[1];
        const description = event.tags.find(t => t[0] === 'description')?.[1];
        const image = event.tags.find(t => t[0] === 'image')?.[1];
        const moderators = event.tags
          .filter(t => t[0] === 'p' && t[3] === 'moderator')
          .map(t => t[1]);
        const relays = event.tags
          .filter(t => t[0] === 'relay')
          .map(t => t[1]);

        return {
          id: event.id,
          pubkey: event.pubkey,
          dTag,
          name: name || dTag,
          description,
          image,
          moderators,
          relays,
          event,
          aTag,
        };
      } catch (error) {
        console.error('Failed to fetch community:', error);
        return null;
      }
    },
    enabled: !!pubkey && !!dTag,
    staleTime: 60000,
  });
}

export function useCommunityPosts(aTag: string) {
  const { nostr } = useNostr();

  // Parse aTag: "34550:pubkey:dTag"
  const [kind, pubkey, dTag] = aTag.split(':');

  return useQuery<NostrEvent[]>({
    queryKey: ['community-posts', aTag],
    queryFn: async ({ signal }) => {
      if (!pubkey || !dTag) return [];

      try {
        // Fetch kind 1111 posts (NIP-72 standard) and kind 1 posts (backwards compatibility)
        const events = await nostr.query(
          [
            { kinds: [1111], '#a': [aTag], limit: 100 },
            { kinds: [1], '#a': [aTag], limit: 100 }
          ],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
        );

        return events.sort((a, b) => b.created_at - a.created_at);
      } catch (error) {
        console.error('Failed to fetch community posts:', error);
        return [];
      }
    },
    enabled: !!pubkey && !!dTag,
    staleTime: 30000,
  });
}

// Hook to get communities the user is a member of (has posted to or moderates)
export function useUserCommunities() {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();

  return useQuery<Set<string>>({
    queryKey: ['user-communities', user?.pubkey],
    queryFn: async ({ signal }) => {
      if (!user) return new Set<string>();

      try {
        // Fetch user's posts that have community tags (kinds 1111 and 1)
        const events = await nostr.query(
          [
            { kinds: [1111, 1], authors: [user.pubkey], limit: 500 }
          ],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
        );

        // Extract all community a tags from user's posts
        const communityATags = new Set<string>();
        events.forEach(event => {
          event.tags.forEach(tag => {
            if (tag[0] === 'a' && tag[1]?.startsWith('34550:')) {
              communityATags.add(tag[1]);
            }
          });
        });

        return communityATags;
      } catch (error) {
        console.error('Failed to fetch user communities:', error);
        return new Set<string>();
      }
    },
    enabled: !!user,
    staleTime: 60000,
  });
}
