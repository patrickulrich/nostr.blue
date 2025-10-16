import { type NostrEvent, type NostrFilter } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useCurrentUser } from './useCurrentUser';

export interface ListItem {
  type: 'p' | 'e' | 'a' | 't' | 'r' | 'word' | 'relay' | 'emoji' | 'group';
  value: string;
  relay?: string;
  marker?: string;
  extra?: string;
}

export interface ListMetadata {
  title?: string;
  description?: string;
  image?: string;
}

/**
 * Generic hook to manage Nostr lists (NIP-51)
 * Supports both standard lists (replaceable) and sets (parameterized replaceable)
 */
export function useLists(kind: number, dTag?: string) {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const queryClient = useQueryClient();
  const userPubkey = user?.pubkey;

  // Determine if this is a parameterized replaceable event (set)
  const isSet = kind >= 30000 && kind < 40000;

  // Fetch the list event
  const { data: listEvent, isLoading } = useQuery<NostrEvent | null>({
    queryKey: ['list', kind, dTag, userPubkey],
    queryFn: async ({ signal }) => {
      if (!userPubkey) return null;

      try {
        const filter: NostrFilter = {
          kinds: [kind],
          authors: [userPubkey],
          limit: 1,
        };

        // Add 'd' tag filter for sets
        if (isSet && dTag) {
          filter['#d'] = [dTag];
        }

        const events = await nostr.query(
          [filter],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
        );

        // Return the most recent list event
        return events.sort((a, b) => b.created_at - a.created_at)[0] || null;
      } catch (error) {
        console.error('Failed to fetch list:', error);
        return null;
      }
    },
    enabled: !!userPubkey,
    staleTime: 30000,
  });

  // Parse list items from the event tags
  const items: ListItem[] = listEvent?.tags
    .filter(tag => tag[0] !== 'd' && tag[0] !== 'title' && tag[0] !== 'description' && tag[0] !== 'image')
    .map(tag => ({
      type: tag[0] as ListItem['type'],
      value: tag[1] || '',
      relay: tag[2],
      marker: tag[3],
      extra: tag[4],
    })) || [];

  // Parse metadata from the event tags
  const metadata: ListMetadata = {
    title: listEvent?.tags.find(tag => tag[0] === 'title')?.[1],
    description: listEvent?.tags.find(tag => tag[0] === 'description')?.[1],
    image: listEvent?.tags.find(tag => tag[0] === 'image')?.[1],
  };

  // Check if an item is in the list
  const hasItem = (type: string, value: string): boolean => {
    return items.some(item => item.type === type && item.value === value);
  };

  // Add an item to the list
  const addItem = useMutation({
    mutationFn: async (item: ListItem) => {
      if (!user) throw new Error('User not logged in');

      const existingTags = listEvent?.tags || [];
      const newTag: string[] = [item.type, item.value];

      if (item.relay) newTag.push(item.relay);
      if (item.marker) newTag.push(item.marker);
      if (item.extra) newTag.push(item.extra);

      // Check if item already exists
      const alreadyExists = existingTags.some(tag =>
        tag[0] === item.type && tag[1] === item.value
      );

      if (alreadyExists) {
        return null; // Silently skip
      }

      // Build tags array
      const tags = [...existingTags, newTag];

      // Add 'd' tag for sets
      if (isSet && dTag && !tags.some(tag => tag[0] === 'd')) {
        tags.unshift(['d', dTag]);
      }

      // Create and sign the list event
      const signedEvent = await user.signer.signEvent({
        kind,
        created_at: Math.floor(Date.now() / 1000),
        tags,
        content: listEvent?.content || '',
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['list', kind, dTag, userPubkey] });
    },
    onError: (error) => {
      console.error('Failed to add item to list:', error);
    },
  });

  // Remove an item from the list
  const removeItem = useMutation({
    mutationFn: async (item: Pick<ListItem, 'type' | 'value'>) => {
      if (!user) throw new Error('User not logged in');
      if (!listEvent) return null;

      // Filter out the item to remove
      const newTags = listEvent.tags.filter(tag => {
        return !(tag[0] === item.type && tag[1] === item.value);
      });

      // Create and sign the list event
      const signedEvent = await user.signer.signEvent({
        kind,
        created_at: Math.floor(Date.now() / 1000),
        tags: newTags,
        content: listEvent.content || '',
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['list', kind, dTag, userPubkey] });
    },
    onError: (error) => {
      console.error('Failed to remove item from list:', error);
    },
  });

  // Update list metadata (for sets)
  const updateMetadata = useMutation({
    mutationFn: async (newMetadata: Partial<ListMetadata>) => {
      if (!user) throw new Error('User not logged in');

      const existingTags = listEvent?.tags.filter(
        tag => tag[0] !== 'title' && tag[0] !== 'description' && tag[0] !== 'image'
      ) || [];

      const metaTags: string[][] = [];
      if (newMetadata.title) metaTags.push(['title', newMetadata.title]);
      if (newMetadata.description) metaTags.push(['description', newMetadata.description]);
      if (newMetadata.image) metaTags.push(['image', newMetadata.image]);

      const tags = [...metaTags, ...existingTags];

      // Add 'd' tag for sets
      if (isSet && dTag && !tags.some(tag => tag[0] === 'd')) {
        tags.unshift(['d', dTag]);
      }

      // Create and sign the list event
      const signedEvent = await user.signer.signEvent({
        kind,
        created_at: Math.floor(Date.now() / 1000),
        tags,
        content: listEvent?.content || '',
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['list', kind, dTag, userPubkey] });
    },
  });

  // Toggle an item in the list
  const toggleItem = useMutation({
    mutationFn: async (item: ListItem) => {
      const exists = hasItem(item.type, item.value);

      if (exists) {
        return await removeItem.mutateAsync({ type: item.type, value: item.value });
      } else {
        return await addItem.mutateAsync(item);
      }
    },
  });

  return {
    listEvent,
    items,
    metadata,
    isLoading,
    hasItem,
    addItem,
    removeItem,
    updateMetadata,
    toggleItem,
  };
}
