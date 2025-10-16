import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useNostrPublish } from './useNostrPublish';
import { useCurrentUser } from './useCurrentUser';

export interface RepostData {
  count: number;
  userReposted: boolean;
  userRepostId?: string;
}

/**
 * Hook to query and manage reposts (Kind 6) for a specific event
 */
export function useReposts(eventId: string | undefined) {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const publish = useNostrPublish();
  const queryClient = useQueryClient();

  // Query all reposts for the event
  const { data, isLoading } = useQuery<NostrEvent[]>({
    queryKey: ['reposts', eventId],
    queryFn: async ({ signal }) => {
      if (!eventId) return [];

      const reposts = await nostr.query(
        [{ kinds: [6], '#e': [eventId], limit: 500 }],
        { signal: AbortSignal.any([signal, AbortSignal.timeout(3000)]) }
      );

      return reposts;
    },
    enabled: !!eventId,
    staleTime: 30000,
  });

  // Calculate repost data
  const repostData: RepostData = {
    count: data?.length || 0,
    userReposted: !!data?.some((r) => r.pubkey === user?.pubkey),
    userRepostId: data?.find((r) => r.pubkey === user?.pubkey)?.id,
  };

  // Mutation to add a repost
  const addRepost = useMutation({
    mutationFn: async (targetEvent: NostrEvent) => {
      if (!user) throw new Error('User not logged in');

      const event = await publish.mutateAsync({
        kind: 6,
        content: JSON.stringify(targetEvent),
        tags: [
          ['e', targetEvent.id],
          ['p', targetEvent.pubkey],
        ],
        created_at: Math.floor(Date.now() / 1000),
      });

      return event;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['reposts', eventId] });
    },
  });

  // Mutation to remove a repost (by publishing a deletion event)
  const removeRepost = useMutation({
    mutationFn: async () => {
      if (!user || !repostData.userRepostId) {
        throw new Error('No repost to remove');
      }

      const event = await publish.mutateAsync({
        kind: 5,
        content: '',
        tags: [['e', repostData.userRepostId]],
        created_at: Math.floor(Date.now() / 1000),
      });

      return event;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['reposts', eventId] });
    },
  });

  return {
    ...repostData,
    isLoading,
    addRepost,
    removeRepost,
  };
}
