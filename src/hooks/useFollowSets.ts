import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useCurrentUser } from './useCurrentUser';
import { encryptPrivateItems, decryptPrivateItems, isEncrypted } from '@/lib/nip44';

const FOLLOW_SETS_KIND = 30000;

export interface FollowSetMember {
  pubkey: string;
  relay?: string;
  isPrivate?: boolean; // For mixed mode
}

export interface FollowSet {
  id: string; // The 'd' tag value
  title: string;
  description?: string;
  image?: string;
  pubkeys: string[]; // All pubkeys (public + private combined)
  publicPubkeys: string[]; // Only public pubkeys
  privatePubkeys: string[]; // Only private pubkeys
  members: FollowSetMember[]; // Detailed member info with privacy status
  isPrivate: boolean; // True if list has encrypted content
  event: NostrEvent;
}

/**
 * Hook to manage follow sets (Kind 30000)
 * Categorized groups of users for different timelines
 */
export function useFollowSets() {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const queryClient = useQueryClient();
  const userPubkey = user?.pubkey;

  // Fetch all follow sets for the current user
  const { data: followSets = [], isLoading } = useQuery<FollowSet[]>({
    queryKey: ['follow-sets', userPubkey],
    queryFn: async ({ signal }) => {
      if (!userPubkey || !user) return [];

      try {
        const events = await nostr.query(
          [{ kinds: [FOLLOW_SETS_KIND], authors: [userPubkey] }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
        );

        // Parse events into FollowSet objects
        const sets: FollowSet[] = await Promise.all(events.map(async event => {
          const dTag = event.tags.find(tag => tag[0] === 'd')?.[1] || '';
          const title = event.tags.find(tag => tag[0] === 'title')?.[1] || dTag || 'Untitled';
          const description = event.tags.find(tag => tag[0] === 'description')?.[1];
          const image = event.tags.find(tag => tag[0] === 'image')?.[1];

          // Get public pubkeys from tags
          const publicPubkeys = event.tags
            .filter(tag => tag[0] === 'p')
            .map(tag => tag[1]);

          // Try to decrypt private items if content exists
          let privatePubkeys: string[] = [];
          let privateMembers: FollowSetMember[] = [];

          if (event.content && isEncrypted(event.content)) {
            try {
              const privateTags = await decryptPrivateItems(event.content, userPubkey, user.signer);
              // Extract private pubkeys
              privatePubkeys = privateTags
                .filter(tag => tag[0] === 'p')
                .map(tag => tag[1]);

              // Create private members with details
              privateMembers = privateTags
                .filter(tag => tag[0] === 'p')
                .map(tag => ({
                  pubkey: tag[1],
                  relay: tag[2],
                  isPrivate: true,
                }));
            } catch (error) {
              console.error('Failed to decrypt private items for list:', dTag, error);
            }
          }

          // Create public members
          const publicMembers: FollowSetMember[] = event.tags
            .filter(tag => tag[0] === 'p')
            .map(tag => ({
              pubkey: tag[1],
              relay: tag[2],
              isPrivate: false,
            }));

          // Combine all pubkeys and members
          const allPubkeys = [...publicPubkeys, ...privatePubkeys];
          const allMembers = [...publicMembers, ...privateMembers];

          return {
            id: dTag,
            title,
            description,
            image,
            pubkeys: allPubkeys,
            publicPubkeys,
            privatePubkeys,
            members: allMembers,
            isPrivate: !!(event.content && isEncrypted(event.content)),
            event,
          };
        }));

        // Sort by created_at descending
        return sets.sort((a, b) => b.event.created_at - a.event.created_at);
      } catch (error) {
        console.error('Failed to fetch follow sets:', error);
        return [];
      }
    },
    enabled: !!userPubkey && !!user,
    staleTime: 30000,
  });

  // Get a specific follow set by ID
  const getFollowSet = (id: string): FollowSet | undefined => {
    return followSets.find(set => set.id === id);
  };

  // Create a new follow set
  const createFollowSet = useMutation({
    mutationFn: async ({
      id,
      title,
      description,
      image,
      pubkeys = [],
      isPrivate = false,
    }: {
      id: string;
      title: string;
      description?: string;
      image?: string;
      pubkeys?: string[];
      isPrivate?: boolean;
    }) => {
      if (!user) throw new Error('User not logged in');

      // Build tags (always include metadata, privacy only affects member list)
      const tags: string[][] = [
        ['d', id],
        ['title', title],
      ];

      if (description) tags.push(['description', description]);
      if (image) tags.push(['image', image]);

      let content = '';

      if (isPrivate) {
        // All pubkeys go in encrypted content
        const privateTags: string[][] = pubkeys.map(pubkey => ['p', pubkey]);
        content = await encryptPrivateItems(privateTags, user.pubkey, user.signer);
      } else {
        // All pubkeys go in public tags
        pubkeys.forEach(pubkey => {
          tags.push(['p', pubkey]);
        });
      }

      // Create and sign the event
      const signedEvent = await user.signer.signEvent({
        kind: FOLLOW_SETS_KIND,
        created_at: Math.floor(Date.now() / 1000),
        tags,
        content,
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['follow-sets', userPubkey] });
    },
    onError: (error) => {
      console.error('Failed to create follow set:', error);
    },
  });

  // Update a follow set's metadata
  const updateFollowSet = useMutation({
    mutationFn: async ({
      id,
      title,
      description,
      image,
    }: {
      id: string;
      title?: string;
      description?: string;
      image?: string;
    }) => {
      if (!user) throw new Error('User not logged in');

      const existingSet = getFollowSet(id);
      if (!existingSet) throw new Error('Follow set not found');

      // Build tags preserving existing public pubkeys
      const tags: string[][] = [['d', id]];

      if (title) tags.push(['title', title]);
      if (description) tags.push(['description', description]);
      if (image) tags.push(['image', image]);

      // Add existing public pubkeys
      existingSet.publicPubkeys.forEach(pubkey => {
        tags.push(['p', pubkey]);
      });

      // Preserve encrypted private content if it exists
      let content = '';
      if (existingSet.isPrivate && existingSet.privatePubkeys.length > 0) {
        const privateTags: string[][] = existingSet.privatePubkeys.map(pk => ['p', pk]);
        content = await encryptPrivateItems(privateTags, user.pubkey, user.signer);
      }

      // Create and sign the event
      const signedEvent = await user.signer.signEvent({
        kind: FOLLOW_SETS_KIND,
        created_at: Math.floor(Date.now() / 1000),
        tags,
        content,
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['follow-sets', userPubkey] });
    },
  });

  // Add a user to a follow set
  const addToFollowSet = useMutation({
    mutationFn: async ({
      listId,
      pubkey,
      relay,
      isPrivate
    }: {
      listId: string;
      pubkey: string;
      relay?: string;
      isPrivate?: boolean; // If undefined, uses list's default privacy setting
    }) => {
      if (!user) throw new Error('User not logged in');

      const existingSet = getFollowSet(listId);
      if (!existingSet) throw new Error('Follow set not found');

      // Check if already in the list
      if (existingSet.pubkeys.includes(pubkey)) {
        return null; // Silently skip
      }

      // Determine if this member should be private
      const memberIsPrivate = isPrivate !== undefined ? isPrivate : existingSet.isPrivate;

      // Rebuild metadata tags
      const tags: string[][] = [['d', listId]];
      if (existingSet.title) tags.push(['title', existingSet.title]);
      if (existingSet.description) tags.push(['description', existingSet.description]);
      if (existingSet.image) tags.push(['image', existingSet.image]);

      // Add existing public pubkeys
      existingSet.publicPubkeys.forEach(pk => {
        tags.push(['p', pk]);
      });

      // Add new pubkey to public tags if not private
      if (!memberIsPrivate) {
        const newTag = relay ? ['p', pubkey, relay] : ['p', pubkey];
        tags.push(newTag);
      }

      // Handle private content
      let content = '';
      const privatePubkeys = [...existingSet.privatePubkeys];

      if (memberIsPrivate) {
        privatePubkeys.push(pubkey);
      }

      if (privatePubkeys.length > 0) {
        const privateTags: string[][] = privatePubkeys.map(pk =>
          pk === pubkey && relay ? ['p', pk, relay] : ['p', pk]
        );
        content = await encryptPrivateItems(privateTags, user.pubkey, user.signer);
      }

      // Create and sign the event
      const signedEvent = await user.signer.signEvent({
        kind: FOLLOW_SETS_KIND,
        created_at: Math.floor(Date.now() / 1000),
        tags,
        content,
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['follow-sets', userPubkey] });
    },
  });

  // Remove a user from a follow set
  const removeFromFollowSet = useMutation({
    mutationFn: async ({ listId, pubkey }: { listId: string; pubkey: string }) => {
      if (!user) throw new Error('User not logged in');

      const existingSet = getFollowSet(listId);
      if (!existingSet) throw new Error('Follow set not found');

      // Rebuild metadata tags
      const tags: string[][] = [['d', listId]];
      if (existingSet.title) tags.push(['title', existingSet.title]);
      if (existingSet.description) tags.push(['description', existingSet.description]);
      if (existingSet.image) tags.push(['image', existingSet.image]);

      // Add public pubkeys except the one to remove
      existingSet.publicPubkeys
        .filter(pk => pk !== pubkey)
        .forEach(pk => {
          tags.push(['p', pk]);
        });

      // Handle private content - remove from private list if it's there
      let content = '';
      const privatePubkeys = existingSet.privatePubkeys.filter(pk => pk !== pubkey);

      if (privatePubkeys.length > 0) {
        const privateTags: string[][] = privatePubkeys.map(pk => ['p', pk]);
        content = await encryptPrivateItems(privateTags, user.pubkey, user.signer);
      }

      // Create and sign the event
      const signedEvent = await user.signer.signEvent({
        kind: FOLLOW_SETS_KIND,
        created_at: Math.floor(Date.now() / 1000),
        tags,
        content,
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['follow-sets', userPubkey] });
    },
  });

  // Delete a follow set
  const deleteFollowSet = useMutation({
    mutationFn: async (listId: string) => {
      if (!user) throw new Error('User not logged in');

      const existingSet = getFollowSet(listId);
      if (!existingSet) throw new Error('Follow set not found');

      // Create a deletion event (Kind 5)
      const signedEvent = await user.signer.signEvent({
        kind: 5,
        created_at: Math.floor(Date.now() / 1000),
        tags: [['e', existingSet.event.id]],
        content: 'Deleted follow set',
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['follow-sets', userPubkey] });
    },
  });

  // Toggle privacy of a follow set (convert all members to public or private)
  const toggleListPrivacy = useMutation({
    mutationFn: async ({ listId, isPrivate }: { listId: string; isPrivate: boolean }) => {
      if (!user) throw new Error('User not logged in');

      const existingSet = getFollowSet(listId);
      if (!existingSet) throw new Error('Follow set not found');

      // Rebuild metadata tags
      const tags: string[][] = [['d', listId]];
      if (existingSet.title) tags.push(['title', existingSet.title]);
      if (existingSet.description) tags.push(['description', existingSet.description]);
      if (existingSet.image) tags.push(['image', existingSet.image]);

      let content = '';

      if (isPrivate) {
        // Move all pubkeys to encrypted content
        const privateTags: string[][] = existingSet.pubkeys.map(pk => ['p', pk]);
        content = await encryptPrivateItems(privateTags, user.pubkey, user.signer);
      } else {
        // Move all pubkeys to public tags
        existingSet.pubkeys.forEach(pk => {
          tags.push(['p', pk]);
        });
      }

      // Create and sign the event
      const signedEvent = await user.signer.signEvent({
        kind: FOLLOW_SETS_KIND,
        created_at: Math.floor(Date.now() / 1000),
        tags,
        content,
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['follow-sets', userPubkey] });
    },
  });

  // Toggle privacy of a specific member (for mixed mode)
  const toggleMemberPrivacy = useMutation({
    mutationFn: async ({ listId, pubkey }: { listId: string; pubkey: string }) => {
      if (!user) throw new Error('User not logged in');

      const existingSet = getFollowSet(listId);
      if (!existingSet) throw new Error('Follow set not found');

      const isCurrentlyPrivate = existingSet.privatePubkeys.includes(pubkey);

      // Rebuild metadata tags
      const tags: string[][] = [['d', listId]];
      if (existingSet.title) tags.push(['title', existingSet.title]);
      if (existingSet.description) tags.push(['description', existingSet.description]);
      if (existingSet.image) tags.push(['image', existingSet.image]);

      let publicPubkeys: string[];
      let privatePubkeys: string[];

      if (isCurrentlyPrivate) {
        // Move from private to public
        publicPubkeys = [...existingSet.publicPubkeys, pubkey];
        privatePubkeys = existingSet.privatePubkeys.filter(pk => pk !== pubkey);
      } else {
        // Move from public to private
        publicPubkeys = existingSet.publicPubkeys.filter(pk => pk !== pubkey);
        privatePubkeys = [...existingSet.privatePubkeys, pubkey];
      }

      // Add public pubkeys to tags
      publicPubkeys.forEach(pk => {
        tags.push(['p', pk]);
      });

      // Encrypt private pubkeys in content
      let content = '';
      if (privatePubkeys.length > 0) {
        const privateTags: string[][] = privatePubkeys.map(pk => ['p', pk]);
        content = await encryptPrivateItems(privateTags, user.pubkey, user.signer);
      }

      // Create and sign the event
      const signedEvent = await user.signer.signEvent({
        kind: FOLLOW_SETS_KIND,
        created_at: Math.floor(Date.now() / 1000),
        tags,
        content,
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['follow-sets', userPubkey] });
    },
  });

  return {
    followSets,
    isLoading,
    getFollowSet,
    createFollowSet,
    updateFollowSet,
    addToFollowSet,
    removeFromFollowSet,
    deleteFollowSet,
    toggleListPrivacy,
    toggleMemberPrivacy,
  };
}
