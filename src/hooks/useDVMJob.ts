import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { useCurrentUser } from './useCurrentUser';

export interface DVMJobRequest {
  kind: number; // 5000-5999
  inputs?: Array<{
    data: string;
    type: 'url' | 'event' | 'job' | 'text';
    relay?: string;
    marker?: string;
  }>;
  params?: Record<string, string>;
  outputType?: string;
  bid?: number; // millisats
  targetPubkey?: string; // Specific DVM to request from
}

export interface DVMJobResult {
  id: string;
  jobRequestId: string;
  dvmPubkey: string;
  content: string;
  status?: 'success' | 'error' | 'partial';
  amount?: number;
  bolt11?: string;
  event: NostrEvent;
  createdAt: number;
}

export interface DVMJobFeedback {
  id: string;
  jobRequestId: string;
  dvmPubkey: string;
  status: 'payment-required' | 'processing' | 'error' | 'success' | 'partial';
  message?: string;
  amount?: number;
  bolt11?: string;
  event: NostrEvent;
}

export function useDVMJob() {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const queryClient = useQueryClient();

  // Submit a DVM job request
  const submitJob = useMutation({
    mutationFn: async (request: DVMJobRequest) => {
      if (!user) throw new Error('User not logged in');

      const tags: string[][] = [];

      // Add input tags
      if (request.inputs) {
        request.inputs.forEach(input => {
          const tag = ['i', input.data, input.type];
          if (input.relay) tag.push(input.relay);
          if (input.marker) tag.push(input.marker);
          tags.push(tag);
        });
      }

      // Add param tags
      if (request.params) {
        Object.entries(request.params).forEach(([key, value]) => {
          tags.push(['param', key, value]);
        });
      }

      // Add output type
      if (request.outputType) {
        tags.push(['output', request.outputType]);
      }

      // Add bid
      if (request.bid) {
        tags.push(['bid', request.bid.toString()]);
      }

      // Add target DVM pubkey
      if (request.targetPubkey) {
        tags.push(['p', request.targetPubkey]);
      }

      // Create and sign the job request
      const signedEvent = await user.signer.signEvent({
        kind: request.kind,
        created_at: Math.floor(Date.now() / 1000),
        tags,
        content: '',
      });

      await nostr.event(signedEvent, { signal: AbortSignal.timeout(5000) });

      return signedEvent;
    },
    onError: (error) => {
      console.error('Failed to submit DVM job:', error);
    },
  });

  // Query job results for a specific job request
  const useJobResults = (jobRequestId: string | null, resultKind: number) => {
    return useQuery<DVMJobResult[]>({
      queryKey: ['dvm-job-results', jobRequestId, resultKind],
      queryFn: async ({ signal }) => {
        if (!jobRequestId) return [];

        try {
          const events = await nostr.query(
            [{ kinds: [resultKind], '#e': [jobRequestId], limit: 50 }],
            { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
          );

          return events.map(event => ({
            id: event.id,
            jobRequestId,
            dvmPubkey: event.pubkey,
            content: event.content,
            amount: parseInt(event.tags.find(t => t[0] === 'amount')?.[1] || '0'),
            bolt11: event.tags.find(t => t[0] === 'amount')?.[2],
            event,
            createdAt: event.created_at,
          })).sort((a, b) => b.createdAt - a.createdAt);
        } catch (error) {
          console.error('Failed to fetch job results:', error);
          return [];
        }
      },
      enabled: !!jobRequestId,
      staleTime: 5000,
      refetchInterval: 10000, // Poll every 10 seconds for new results
    });
  };

  // Query job feedback for a specific job request
  const useJobFeedback = (jobRequestId: string | null) => {
    return useQuery<DVMJobFeedback[]>({
      queryKey: ['dvm-job-feedback', jobRequestId],
      queryFn: async ({ signal }) => {
        if (!jobRequestId) return [];

        try {
          const events = await nostr.query(
            [{ kinds: [7000], '#e': [jobRequestId], limit: 20 }],
            { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
          );

          return events.map(event => {
            const statusTag = event.tags.find(t => t[0] === 'status');
            return {
              id: event.id,
              jobRequestId,
              dvmPubkey: event.pubkey,
              status: (statusTag?.[1] as DVMJobFeedback['status']) || 'processing',
              message: statusTag?.[2],
              amount: parseInt(event.tags.find(t => t[0] === 'amount')?.[1] || '0'),
              bolt11: event.tags.find(t => t[0] === 'amount')?.[2],
              event,
            };
          }).sort((a, b) => b.event.created_at - a.event.created_at);
        } catch (error) {
          console.error('Failed to fetch job feedback:', error);
          return [];
        }
      },
      enabled: !!jobRequestId,
      staleTime: 5000,
      refetchInterval: 10000, // Poll every 10 seconds
    });
  };

  // Query all results from a specific DVM for discovery feeds
  const useDVMFeed = (dvmPubkey: string, requestKind: number, resultKind: number) => {
    return useQuery<NostrEvent[]>({
      queryKey: ['dvm-feed', dvmPubkey, requestKind, resultKind],
      queryFn: async ({ signal }) => {
        console.log('[useDVMFeed] Querying DVM:', dvmPubkey, 'kind:', resultKind);
        try {
          // Get results from this DVM
          const events = await nostr.query(
            [{ kinds: [resultKind], authors: [dvmPubkey], limit: 50 }],
            { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
          );

          console.log('[useDVMFeed] Received', events.length, 'DVM result events');
          if (events.length > 0) {
            console.log('[useDVMFeed] Sample event:', events[0]);
          }

          return events.sort((a, b) => b.created_at - a.created_at);
        } catch (error) {
          console.error('[useDVMFeed] Failed to fetch DVM feed:', error);
          return [];
        }
      },
      staleTime: 30000,
    });
  };

  return {
    submitJob,
    useJobResults,
    useJobFeedback,
    useDVMFeed,
  };
}
