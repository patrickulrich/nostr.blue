import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useCurrentUser } from './useCurrentUser';
import { useNostrPublish } from './useNostrPublish';

/**
 * Hook to manage following/contact lists (Kind 3)
 */
export function useFollowing(pubkey?: string) {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const publish = useNostrPublish();
  const queryClient = useQueryClient();

  // Use current user's pubkey if no pubkey provided
  const targetPubkey = pubkey || user?.pubkey;

  // Query the contact list
  const { data: contactListEvent, isLoading } = useQuery<NostrEvent | null>({
    queryKey: ['contacts', targetPubkey],
    queryFn: async ({ signal }) => {
      if (!targetPubkey) return null;

      const [event] = await nostr.query(
        [{ kinds: [3], authors: [targetPubkey], limit: 1 }],
        { signal: AbortSignal.any([signal, AbortSignal.timeout(3000)]) }
      );

      return event || null;
    },
    enabled: !!targetPubkey,
    staleTime: 60000, // Cache for 1 minute
  });

  // Extract following list from contact event
  const following = contactListEvent?.tags
    .filter(([tag]) => tag === 'p')
    .map(([, pubkey]) => pubkey) || [];

  // Check if current user is following a specific pubkey
  const isFollowing = (checkPubkey: string): boolean => {
    return following.includes(checkPubkey);
  };

  // Mutation to follow a user
  const follow = useMutation({
    mutationFn: async (followPubkey: string) => {
      if (!user) throw new Error('User not logged in');

      // Get existing contacts
      const existingTags = contactListEvent?.tags || [];

      // Check if already following
      if (existingTags.some(([tag, pk]) => tag === 'p' && pk === followPubkey)) {
        throw new Error('Already following this user');
      }

      // Add new follow
      const newTags = [...existingTags, ['p', followPubkey]];

      const event = await publish.mutateAsync({
        kind: 3,
        content: contactListEvent?.content || '',
        tags: newTags,
        created_at: Math.floor(Date.now() / 1000),
      });

      return event;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['contacts', user?.pubkey] });
    },
  });

  // Mutation to unfollow a user
  const unfollow = useMutation({
    mutationFn: async (unfollowPubkey: string) => {
      if (!user) throw new Error('User not logged in');

      // Get existing contacts and remove the unfollowed pubkey
      const existingTags = contactListEvent?.tags || [];
      const newTags = existingTags.filter(
        ([tag, pk]) => !(tag === 'p' && pk === unfollowPubkey)
      );

      const event = await publish.mutateAsync({
        kind: 3,
        content: contactListEvent?.content || '',
        tags: newTags,
        created_at: Math.floor(Date.now() / 1000),
      });

      return event;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['contacts', user?.pubkey] });
    },
  });

  return {
    following,
    followingCount: following.length,
    isFollowing,
    isLoading,
    follow,
    unfollow,
  };
}
