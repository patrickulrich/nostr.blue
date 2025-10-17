import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery, useMutation } from '@tanstack/react-query';
import { useCurrentUser } from './useCurrentUser';
import { nip19 } from 'nostr-tools';

/**
 * Safely parses an amount string to a number.
 * Returns undefined for invalid or missing values to distinguish from zero.
 * @param v - Amount string to parse
 * @returns Parsed number if valid, undefined otherwise
 */
function parseAmountSafe(v?: string): number | undefined {
  if (!v) return undefined;
  const s = v.trim();
  if (!/^\d+$/.test(s)) return undefined;
  const n = Number(s);
  return Number.isSafeInteger(n) ? n : undefined;
}

/**
 * Normalizes a Nostr event ID from NIP-19 format (note1/nevent1) to hex.
 * @param id - Event ID in hex or NIP-19 format
 * @returns Hex event ID
 */
function normalizeEventId(id: string): string {
  if (!id) return id;
  if (id.startsWith('note1')) {
    try {
      return nip19.decode(id).data as string;
    } catch {
      return id;
    }
  }
  if (id.startsWith('nevent1')) {
    try {
      const decoded = nip19.decode(id);
      if (decoded.type === 'nevent') {
        const eventData = decoded.data as { id: string };
        return typeof eventData.id === 'string' ? eventData.id : id;
      }
      return id;
    } catch {
      return id;
    }
  }
  return id; // assume already hex
}

/**
 * Normalizes a Nostr pubkey from NIP-19 format (npub1/nprofile1) to hex.
 * @param pk - Pubkey in hex or NIP-19 format
 * @returns Hex pubkey
 */
function normalizePubkey(pk: string): string {
  if (!pk) return pk;
  if (pk.startsWith('npub1')) {
    try {
      return nip19.decode(pk).data as string;
    } catch {
      return pk;
    }
  }
  if (pk.startsWith('nprofile1')) {
    try {
      const decoded = nip19.decode(pk);
      if (decoded.type === 'nprofile') {
        const profileData = decoded.data as { pubkey: string };
        return typeof profileData.pubkey === 'string' ? profileData.pubkey : pk;
      }
      return pk;
    } catch {
      return pk;
    }
  }
  return pk; // assume already hex
}

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

/**
 * Hook to manage DVM (Data Vending Machine) job requests and results.
 * Provides functionality to submit jobs, query results, and monitor job feedback.
 * @returns Object containing job submission mutation and query hooks for results and feedback
 */
export function useDVMJob() {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();

  // Submit a DVM job request
  const submitJob = useMutation({
    mutationFn: async (request: DVMJobRequest) => {
      if (!user) throw new Error('User not logged in');

      // Validate DVM request kind
      if (request.kind < 5000 || request.kind > 5999) {
        throw new Error(`Invalid DVM job kind: ${request.kind}. Must be between 5000-5999.`);
      }

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

      // Add target DVM pubkey (hex)
      if (request.targetPubkey) {
        tags.push(['p', normalizePubkey(request.targetPubkey)]);
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
          const idHex = normalizeEventId(jobRequestId);
          const events = await nostr.query(
            [{ kinds: [resultKind], '#e': [idHex], limit: 50 }],
            { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
          );

          return events.map(event => {
            const amountTag = event.tags.find(t => t[0] === 'amount');
            const bolt11Tag = event.tags.find(t => t[0] === 'bolt11');
            return {
              id: event.id,
              jobRequestId,
              dvmPubkey: event.pubkey,
              content: event.content,
              amount: parseAmountSafe(amountTag?.[1]),
              bolt11: bolt11Tag?.[1],
              event,
              createdAt: event.created_at,
            };
          }).sort((a, b) => b.createdAt - a.createdAt);
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
          const idHex = normalizeEventId(jobRequestId);
          const events = await nostr.query(
            [{ kinds: [7000], '#e': [idHex], limit: 20 }],
            { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
          );

          return events.map(event => {
            const statusTag = event.tags.find(t => t[0] === 'status');
            const amountTag = event.tags.find(t => t[0] === 'amount');
            const bolt11Tag = event.tags.find(t => t[0] === 'bolt11');
            return {
              id: event.id,
              jobRequestId,
              dvmPubkey: event.pubkey,
              status: (statusTag?.[1] as DVMJobFeedback['status']) || 'processing',
              message: statusTag?.[2],
              amount: parseAmountSafe(amountTag?.[1]),
              bolt11: bolt11Tag?.[1],
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
        try {
          // Get results from this DVM
          const authorHex = normalizePubkey(dvmPubkey);
          const events = await nostr.query(
            [{ kinds: [resultKind], authors: [authorHex], limit: 50 }],
            { signal: AbortSignal.any([signal, AbortSignal.timeout(10000)]) }
          );

          return events.sort((a, b) => b.created_at - a.created_at);
        } catch (error) {
          console.error('[useDVMFeed] Failed to fetch DVM feed:', error);
          return [];
        }
      },
      staleTime: 30000,
      refetchOnMount: true, // Always refetch when component mounts
    });
  };

  return {
    submitJob,
    useJobResults,
    useJobFeedback,
    useDVMFeed,
  };
}
