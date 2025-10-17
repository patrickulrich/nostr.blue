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
 * Helper to check if a reaction is real (not an optimistic temp reaction).
 * A real reaction has a valid 64-char hex ID and a non-empty signature.
 */
function isRealReaction(r: NostrEvent): boolean {
  return /^[0-9a-f]{64}$/i.test(r.id) && !!r.sig;
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
    userReactionId: data?.find((r) => r.pubkey === user?.pubkey && isRealReaction(r))?.id,
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
    onMutate: async (targetEvent: NostrEvent) => {
      // Cancel any outgoing refetches
      await queryClient.cancelQueries({ queryKey: ['reactions', targetEvent.id] });

      // Snapshot the previous value
      const previousReactions = queryClient.getQueryData<NostrEvent[]>(['reactions', targetEvent.id]);

      // Optimistically update with a temporary reaction
      if (user && targetEvent?.id) {
        const optimisticReaction: NostrEvent = {
          id: 'temp-' + Date.now(),
          pubkey: user.pubkey,
          created_at: Math.floor(Date.now() / 1000),
          kind: 7,
          tags: [['e', targetEvent.id]],
          content: '+',
          sig: '',
        };

        queryClient.setQueryData<NostrEvent[]>(
          ['reactions', targetEvent.id],
          (old) => [...(old || []), optimisticReaction]
        );
      }

      return { previousReactions };
    },
    onError: (_err, _variables, context) => {
      // Rollback on error
      const prev = (context as { previousReactions?: NostrEvent[] })?.previousReactions;
      if (prev) {
        queryClient.setQueryData(['reactions', eventId], prev);
      }
    },
    onSuccess: () => {
      // Invalidate reactions query to refetch real data
      queryClient.invalidateQueries({ queryKey: ['reactions', eventId] });
    },
  });

  // Mutation to remove a reaction (by publishing a deletion event)
  const removeReaction = useMutation({
    mutationFn: async () => {
      if (!user) {
        throw new Error('User not logged in');
      }

      // Get all real reaction IDs by this user
      const currentReactions = queryClient.getQueryData<NostrEvent[]>(['reactions', eventId]) || [];
      const idsToDelete = currentReactions
        .filter(r => r.pubkey === user.pubkey && isRealReaction(r))
        .map(r => r.id);

      if (idsToDelete.length === 0) {
        throw new Error('No reaction to remove');
      }

      // Publish a Kind 5 deletion event with all reaction IDs
      const event = await publish.mutateAsync({
        kind: 5,
        content: '',
        tags: idsToDelete.map(id => ['e', id]),
        created_at: Math.floor(Date.now() / 1000),
      });

      return event;
    },
    onMutate: async () => {
      // Cancel any outgoing refetches
      await queryClient.cancelQueries({ queryKey: ['reactions', eventId] });

      // Snapshot the previous value
      const previousReactions = queryClient.getQueryData<NostrEvent[]>(['reactions', eventId]);

      // Optimistically remove the user's reaction
      if (user) {
        queryClient.setQueryData<NostrEvent[]>(
          ['reactions', eventId],
          (old) => (old || []).filter((r) => r.pubkey !== user.pubkey)
        );
      }

      return { previousReactions };
    },
    onError: (_err, _variables, context) => {
      // Rollback on error
      const prev = (context as { previousReactions?: NostrEvent[] })?.previousReactions;
      if (prev) {
        queryClient.setQueryData(['reactions', eventId], prev);
      }
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
