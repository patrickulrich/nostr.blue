import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery } from '@tanstack/react-query';

const DVM_ANNOUNCEMENT_KIND = 31990;

export interface DVMService {
  id: string;
  pubkey: string;
  name?: string;
  about?: string;
  picture?: string;
  supportedKinds: number[];
  tags: string[];
  handlers: {
    platform: string;
    url: string;
    entityType?: string;
  }[];
  event: NostrEvent;
}

export function useDVMs() {
  const { nostr } = useNostr();

  const { data: dvmEvents, isLoading } = useQuery<NostrEvent[]>({
    queryKey: ['dvms'],
    queryFn: async ({ signal }) => {
      try {
        const events = await nostr.query(
          [{ kinds: [DVM_ANNOUNCEMENT_KIND], limit: 100 }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
        );

        // Sort by created_at descending
        return events.sort((a, b) => b.created_at - a.created_at);
      } catch (error) {
        console.error('Failed to fetch DVM announcements:', error);
        return [];
      }
    },
    staleTime: 60000, // 1 minute
  });

  // Parse DVMs from events
  const dvms: DVMService[] = (dvmEvents || []).map(event => {
    // Parse metadata from content field if present
    let metadata: { name?: string; about?: string; picture?: string } = {};
    if (event.content) {
      try {
        metadata = JSON.parse(event.content);
      } catch {
        // Ignore parsing errors
      }
    }

    // Extract d tag (identifier)
    const dTag = event.tags.find(tag => tag[0] === 'd')?.[1] || event.id;

    // Extract supported kinds
    const supportedKinds = event.tags
      .filter(tag => tag[0] === 'k')
      .map(tag => parseInt(tag[1], 10))
      .filter(k => !isNaN(k));

    // Extract topic tags
    const topicTags = event.tags
      .filter(tag => tag[0] === 't')
      .map(tag => tag[1]);

    // Extract handlers (web, ios, android, etc.)
    const handlers = event.tags
      .filter(tag => ['web', 'ios', 'android', 'desktop'].includes(tag[0]))
      .map(tag => ({
        platform: tag[0],
        url: tag[1],
        entityType: tag[2], // e.g., 'nevent', 'nprofile', etc.
      }));

    return {
      id: dTag,
      pubkey: event.pubkey,
      name: metadata.name,
      about: metadata.about,
      picture: metadata.picture,
      supportedKinds,
      tags: topicTags,
      handlers,
      event,
    };
  });

  // Helper function to filter DVMs by supported kind
  const getDVMsByKind = (kind: number): DVMService[] => {
    return dvms.filter(dvm =>
      dvm.supportedKinds.length === 0 || dvm.supportedKinds.includes(kind)
    );
  };

  // Helper function to filter DVMs by tag
  const getDVMsByTag = (tag: string): DVMService[] => {
    return dvms.filter(dvm => dvm.tags.includes(tag));
  };

  return {
    dvms,
    isLoading,
    getDVMsByKind,
    getDVMsByTag,
  };
}
