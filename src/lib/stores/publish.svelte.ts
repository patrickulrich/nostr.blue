import type { TrustedEvent } from '@welshman/util';
import { makeEvent } from '@welshman/util';
import { createMutation } from '@tanstack/svelte-query';
import { publishThunk } from '@welshman/app';
import { Router } from '@welshman/router';

export interface PublishEventParams {
	kind: number;
	content?: string;
	tags?: string[][];
}

/**
 * TanStack mutation for publishing Nostr events
 * Automatically signs events with the current signer and publishes to configured relays
 *
 * @returns TanStack mutation for event publishing
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { useNostrPublish } from '$lib/stores/publish.svelte';
 *
 *   const publish = useNostrPublish();
 *
 *   function handleSubmit() {
 *     $publish.mutate({
 *       kind: 1,
 *       content: 'Hello Nostr!',
 *       tags: []
 *     });
 *   }
 * </script>
 *
 * <button onclick={handleSubmit} disabled={$publish.isPending}>
 *   {$publish.isPending ? 'Publishing...' : 'Publish'}
 * </button>
 * ```
 */
export function useNostrPublish() {
	return createMutation(() => ({
		mutationFn: async (params: PublishEventParams) => {
			const tags = params.tags ?? [];

			// Add the client tag if it doesn't exist and we're on HTTPS
			if (
				typeof window !== 'undefined' &&
				location.protocol === 'https:' &&
				!tags.some(([name]) => name === 'client')
			) {
				tags.push(['client', location.hostname]);
			}

			// Create event using makeEvent (adds created_at automatically)
			const event = makeEvent(params.kind, {
				content: params.content ?? '',
				tags
			});

			// Use router to determine which relays to publish to
			// This uses the user's write relays (outbox) and mentioned users' read relays
			let relays = Router.get().PublishEvent(event).getUrls();

			// Fallback to default relays if router returns no relays
			// (e.g., user hasn't published a NIP-65 relay list yet)
			if (relays.length === 0) {
				relays = Router.get().getDefaultRelays();
			}

			// Publish to relays (publishThunk will sign and publish)
			const thunk = publishThunk({ event, relays });

			// Return the signed event immediately
			// The thunk handles publishing in the background - we don't need to wait
			return thunk.event as TrustedEvent;
		},
		onError: (error) => {
			console.error('Failed to publish event:', error);
		},
		onSuccess: (data) => {
			console.log('Event published successfully:', data);
		}
	}));
}

/**
 * Simplified publish function for direct use
 * Signs and publishes a Nostr event without TanStack mutation wrapper
 *
 * @param params - Event parameters (kind, content, tags)
 * @returns Promise<TrustedEvent> - The signed and published event
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { publishNostrEvent } from '$lib/stores/publish.svelte';
 *
 *   async function handlePost() {
 *     try {
 *       const event = await publishNostrEvent({
 *         kind: 1,
 *         content: 'Hello Nostr!',
 *         tags: []
 *       });
 *       console.log('Published:', event.id);
 *     } catch (error) {
 *       console.error('Failed to publish:', error);
 *     }
 *   }
 * </script>
 * ```
 */
export async function publishNostrEvent(params: PublishEventParams): Promise<TrustedEvent> {
	const tags = params.tags ?? [];

	// Add the client tag if it doesn't exist and we're on HTTPS
	if (
		typeof window !== 'undefined' &&
		location.protocol === 'https:' &&
		!tags.some(([name]) => name === 'client')
	) {
		tags.push(['client', location.hostname]);
	}

	// Create event using makeEvent (adds created_at automatically)
	const event = makeEvent(params.kind, {
		content: params.content ?? '',
		tags
	});

	// Use router to determine which relays to publish to
	// This uses the user's write relays (outbox) and mentioned users' read relays
	let relays = Router.get().PublishEvent(event).getUrls();

	// Fallback to default relays if router returns no relays
	// (e.g., user hasn't published a NIP-65 relay list yet)
	if (relays.length === 0) {
		relays = Router.get().getDefaultRelays();
	}

	// Publish to relays (publishThunk will sign and publish)
	const thunk = publishThunk({ event, relays });

	// Return the signed event immediately
	// The thunk handles publishing in the background - we don't need to wait
	return thunk.event as TrustedEvent;
}
