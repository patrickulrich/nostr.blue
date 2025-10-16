import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useNostrPublish } from './useNostrPublish';
import { useCurrentUser } from './useCurrentUser';

export interface ReactionData {
  count: number;
  userReacted: boolean;
  userReactionId?: string;
}

/**
 * Hook to query and manage reactions (Kind 7) for a specific event
 */
export function useReactions(eventId: string | undefined) {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const publish = useNostrPublish();
  const queryClient = useQueryClient();

  // Query all reactions for the event
  const { data, isLoading } = useQuery<NostrEvent[]>({
    queryKey: ['reactions', eventId],
    queryFn: async ({ signal }) => {
      if (!eventId) return [];

      const reactions = await nostr.query(
        [{ kinds: [7], '#e': [eventId], limit: 500 }],
        { signal: AbortSignal.any([signal, AbortSignal.timeout(3000)]) }
      );

      return reactions;
    },
    enabled: !!eventId,
    staleTime: 30000,
  });

  // Calculate reaction data
  const reactionData: ReactionData = {
    count: data?.length || 0,
    userReacted: !!data?.some((r) => r.pubkey === user?.pubkey),
    userReactionId: data?.find((r) => r.pubkey === user?.pubkey)?.id,
  };

  // Mutation to add a reaction
  const addReaction = useMutation({
    mutationFn: async (targetEvent: NostrEvent) => {
      if (!user) throw new Error('User not logged in');

      const event = await publish.mutateAsync({
        kind: 7,
        content: '+', // "+" is the standard like reaction
        tags: [
          ['e', targetEvent.id],
          ['p', targetEvent.pubkey],
        ],
        created_at: Math.floor(Date.now() / 1000),
      });

      return event;
    },
    onSuccess: () => {
      // Invalidate reactions query to refetch
      queryClient.invalidateQueries({ queryKey: ['reactions', eventId] });
    },
  });

  // Mutation to remove a reaction (by publishing a deletion event)
  const removeReaction = useMutation({
    mutationFn: async () => {
      if (!user || !reactionData.userReactionId) {
        throw new Error('No reaction to remove');
      }

      // Publish a Kind 5 deletion event
      const event = await publish.mutateAsync({
        kind: 5,
        content: '',
        tags: [['e', reactionData.userReactionId]],
        created_at: Math.floor(Date.now() / 1000),
      });

      return event;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['reactions', eventId] });
    },
  });

  return {
    ...reactionData,
    isLoading,
    addReaction,
    removeReaction,
  };
}
