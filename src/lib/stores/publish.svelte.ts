import type { TrustedEvent, EventTemplate } from '@welshman/util';
import { createMutation } from '@tanstack/svelte-query';
import { signer, pubkey } from '@welshman/app';
import { publishThunk } from '@welshman/app';
import { get } from 'svelte/store';

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
	return createMutation({
		mutationFn: async (template: Omit<EventTemplate, 'pubkey'>) => {
			const currentSigner = get(signer);
			const currentPubkey = get(pubkey);

			if (!currentSigner || !currentPubkey) {
				throw new Error('User is not logged in');
			}

			const tags = template.tags ?? [];

			// Add the client tag if it doesn't exist and we're on HTTPS
			if (
				typeof window !== 'undefined' &&
				location.protocol === 'https:' &&
				!tags.some(([name]) => name === 'client')
			) {
				tags.push(['client', location.hostname]);
			}

			// Create event template
			const eventTemplate: EventTemplate = {
				kind: template.kind,
				content: template.content ?? '',
				tags,
				created_at: template.created_at ?? Math.floor(Date.now() / 1000)
			};

			// Sign the event
			const signedEvent = await currentSigner.sign(eventTemplate);

			// Publish to relays
			const thunk = publishThunk({ event: signedEvent, relays: [] });

			// Wait for publication to complete (with timeout)
			await Promise.race([
				thunk.complete,
				new Promise((_, reject) =>
					setTimeout(() => reject(new Error('Publication timeout')), 5000)
				)
			]);

			// Return the signed event
			return signedEvent as TrustedEvent;
		},
		onError: (error) => {
			console.error('Failed to publish event:', error);
		},
		onSuccess: (data) => {
			console.log('Event published successfully:', data);
		}
	});
}

/**
 * Simplified publish function for direct use
 * Signs and publishes a Nostr event without TanStack mutation wrapper
 *
 * @param template - Event template (without pubkey)
 * @returns Promise<NostrEvent> - The signed and published event
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
export async function publishNostrEvent(
	template: Omit<EventTemplate, 'pubkey'>
): Promise<TrustedEvent> {
	const currentSigner = get(signer);
	const currentPubkey = get(pubkey);

	if (!currentSigner || !currentPubkey) {
		throw new Error('User is not logged in');
	}

	const tags = template.tags ?? [];

	// Add the client tag if it doesn't exist and we're on HTTPS
	if (
		typeof window !== 'undefined' &&
		location.protocol === 'https:' &&
		!tags.some(([name]) => name === 'client')
	) {
		tags.push(['client', location.hostname]);
	}

	// Create event template
	const eventTemplate: EventTemplate = {
		kind: template.kind,
		content: template.content ?? '',
		tags,
		created_at: template.created_at ?? Math.floor(Date.now() / 1000)
	};

	// Sign the event
	const signedEvent = await currentSigner.sign(eventTemplate);

	// Publish to relays
	const thunk = publishThunk({ event: signedEvent, relays: [] });

	// Wait for publication to complete (with timeout)
	await Promise.race([
		thunk.complete,
		new Promise((_, reject) =>
			setTimeout(() => reject(new Error('Publication timeout')), 5000)
		)
	]);

	return signedEvent as TrustedEvent;
}
