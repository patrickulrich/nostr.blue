import type { TrustedEvent } from '@welshman/util';
import { ZAP_RESPONSE } from '@welshman/util';
import { createQuery, useQueryClient } from '@tanstack/svelte-query';
import { load } from '@welshman/net';
import { pubkey, signer } from '@welshman/app';
import { useAuthor } from './author.svelte';
import { appConfig } from './appStore';
import { nip57 } from 'nostr-tools';
import type { Event } from 'nostr-tools';
import type { WebLNProvider } from '@webbtc/webln-types';
import { nwcStore, type NWCConnection } from './nwc.svelte';
import { toastError, toastSuccess } from './toast.svelte';
import { get } from 'svelte/store';

/**
 * Query zap receipts for a Nostr event
 * Returns zap data including counts, totals, and individual zap events
 *
 * @param target - The Nostr event to query zaps for
 * @param webln - WebLN provider instance (optional)
 * @param nwcConnection - NWC connection (optional)
 * @param onZapSuccess - Callback when zap succeeds (optional)
 * @returns Object with zap data and zap function
 *
 * @example
 * ```svelte
 * <script lang="ts">
 *   import { useZaps } from '$lib/stores/zaps.svelte';
 *
 *   let { event } = $props();
 *   const zaps = useZaps(event);
 *
 *   let count = $derived($zaps.data?.zapCount ?? 0);
 *   let total = $derived($zaps.data?.totalSats ?? 0);
 * </script>
 *
 * <div>
 *   {count} zaps ({total} sats)
 *   <button onclick={() => zaps.zap(100, 'Great post!')}>
 *     Zap 100 sats
 *   </button>
 * </div>
 * ```
 */
export function useZaps(
	target: Event | undefined,
	webln?: WebLNProvider | null,
	nwcConnection?: NWCConnection | null,
	onZapSuccess?: () => void
) {
	const queryClient = useQueryClient();

	let isZapping = $state(false);
	let invoice = $state<string | null>(null);

	interface ZapQueryData {
		zapCount: number;
		totalSats: number;
		zaps: TrustedEvent[];
	}

	// Query for zap receipts
	// @ts-expect-error - TanStack Query in Svelte requires createQuery to be called within component context.
	// TODO: Refactor to use createQuery directly in components instead of wrapping in functions.
	const zapQuery = createQuery<TrustedEvent[], Error, ZapQueryData>(() => ({
		queryKey: ['zaps', target?.id] as const,
		staleTime: 30000, // 30 seconds
		refetchInterval: 60000, // 1 minute
		queryFn: async ({ signal }) => {
			if (!target) return [];

			// Query for zap receipts for this specific event
			if (target.kind >= 30000 && target.kind < 40000) {
				// Addressable event
				const identifier = target.tags.find((t) => t[0] === 'd')?.[1] || '';
				const events = await load({
					relays: [],
					filters: [
						{
							kinds: [ZAP_RESPONSE],
							'#a': [`${target.kind}:${target.pubkey}:${identifier}`]
						}
					],
					signal,
				});
				return events;
			} else {
				// Regular event
				const events = await load({
					relays: [],
					filters: [
						{
							kinds: [ZAP_RESPONSE],
							'#e': [target.id]
						}
					],
					signal,
				});
				return events;
			}
		},
		enabled: !!target?.id,
		select: (zapEvents: TrustedEvent[]) => {
			if (!zapEvents || !Array.isArray(zapEvents) || !target) {
				return { zapCount: 0, totalSats: 0, zaps: [] };
			}

			let count = 0;
			let sats = 0;

			zapEvents.forEach((zap) => {
				count++;

				// Try multiple methods to extract the amount:

				// Method 1: amount tag (from zap request, sometimes copied to receipt)
				const amountTag = zap.tags.find(([name]) => name === 'amount')?.[1];
				if (amountTag) {
					const millisats = parseInt(amountTag);
					sats += Math.floor(millisats / 1000);
					return;
				}

				// Method 2: Extract from bolt11 invoice
				const bolt11Tag = zap.tags.find(([name]) => name === 'bolt11')?.[1];
				if (bolt11Tag) {
					try {
						const invoiceSats = nip57.getSatoshisAmountFromBolt11(bolt11Tag);
						sats += invoiceSats;
						return;
					} catch (error) {
						console.warn('Failed to parse bolt11 amount:', error);
					}
				}

				// Method 3: Parse from description (zap request JSON)
				const descriptionTag = zap.tags.find(([name]) => name === 'description')?.[1];
				if (descriptionTag) {
					try {
						const zapRequest = JSON.parse(descriptionTag);
						const requestAmountTag = zapRequest.tags?.find(
							([name]: string[]) => name === 'amount'
						)?.[1];
						if (requestAmountTag) {
							const millisats = parseInt(requestAmountTag);
							sats += Math.floor(millisats / 1000);
							return;
						}
					} catch (error) {
						console.warn('Failed to parse description JSON:', error);
					}
				}

				console.warn('Could not extract amount from zap receipt:', zap.id);
			});

			return { zapCount: count, totalSats: sats, zaps: zapEvents };
		}
	}));

	// Get author for the target event
	const author = target ? useAuthor(target.pubkey) : null;

	async function zap(amount: number, comment: string) {
		if (amount <= 0 || !target) {
			return;
		}

		isZapping = true;
		invoice = null;

		const currentSigner = get(signer);
		const currentPubkey = get(pubkey);

		if (!currentSigner || !currentPubkey) {
			toastError('Login required', 'You must be logged in to send a zap.');
			isZapping = false;
			return;
		}

		try {
			if (!author) {
				toastError('Author not found', 'Could not find the author of this item.');
				isZapping = false;
				return;
			}

			// Get the current author data
			const authorQuery = author;
			const authorData = get(authorQuery).data;

			if (!authorData?.metadata || !authorData?.event) {
				toastError('Author profile not loaded', 'Could not load the author profile.');
				isZapping = false;
				return;
			}
			const { lud06, lud16 } = authorData.metadata || {};

			if (!lud06 && !lud16) {
				toastError('Lightning address not found', 'The author does not have a lightning address configured.');
				isZapping = false;
				return;
			}

			// Get zap endpoint using the old reliable method
			const zapEndpoint = await nip57.getZapEndpoint(authorData.event! as Event);
			if (!zapEndpoint) {
				toastError('Zap endpoint not found', 'Could not find a zap endpoint for the author.');
				isZapping = false;
				return;
			}

			// Create zap request
			const event = target.kind >= 30000 && target.kind < 40000 ? target : target.id;

			const zapAmount = amount * 1000; // convert to millisats

			const config = get(appConfig);

			const zapRequest = nip57.makeZapRequest({
				profile: target.pubkey,
				event: event,
				amount: zapAmount,
				relays: [config.relayUrl],
				comment
			});

			// Sign the zap request
			const signedZapRequest = await currentSigner.sign(zapRequest);

			try {
				const res = await fetch(
					`${zapEndpoint}?amount=${zapAmount}&nostr=${encodeURI(JSON.stringify(signedZapRequest))}`
				);
				const responseData = await res.json();

				if (!res.ok) {
					throw new Error(`HTTP ${res.status}: ${responseData.reason || 'Unknown error'}`);
				}

				const newInvoice = responseData.pr;
				if (!newInvoice || typeof newInvoice !== 'string') {
					throw new Error('Lightning service did not return a valid invoice');
				}

				// Get the current active NWC connection dynamically
				const currentNWCConnection = nwcConnection || nwcStore.getActiveConnection();

				// Try NWC first if available and properly connected
				if (
					currentNWCConnection &&
					currentNWCConnection.connectionString &&
					currentNWCConnection.isConnected
				) {
					try {
						await nwcStore.sendPayment(currentNWCConnection, newInvoice);

						isZapping = false;
						invoice = null;

						toastSuccess('Zap successful!', `You sent ${amount} sats via NWC to the author.`);

						// Invalidate zap queries to refresh counts
						queryClient.invalidateQueries({ queryKey: ['zaps'] });

						onZapSuccess?.();
						return;
					} catch (nwcError) {
						console.error('NWC payment failed, falling back:', nwcError);

						const errorMessage =
							nwcError instanceof Error ? nwcError.message : 'Unknown NWC error';
						toastError('NWC payment failed', `${errorMessage}. Falling back to other payment methods...`);
					}
				}

				// Try WebLN next
				if (webln) {
					try {
						let webLnProvider = webln;
						if (webln.enable && typeof webln.enable === 'function') {
							const enabledProvider = await webln.enable();
							const provider = enabledProvider as WebLNProvider | undefined;
							if (provider) {
								webLnProvider = provider;
							}
						}

						await webLnProvider.sendPayment(newInvoice);

						isZapping = false;
						invoice = null;

						toastSuccess('Zap successful!', `You sent ${amount} sats to the author.`);

						queryClient.invalidateQueries({ queryKey: ['zaps'] });

						onZapSuccess?.();
						return;
					} catch (weblnError) {
						console.error('WebLN payment failed, falling back:', weblnError);

						const errorMessage =
							weblnError instanceof Error ? weblnError.message : 'Unknown WebLN error';
						toastError('WebLN payment failed', `${errorMessage}. Falling back to other payment methods...`);

						invoice = newInvoice;
						isZapping = false;
					}
				} else {
					// Default - show QR code and manual Lightning URI
					invoice = newInvoice;
					isZapping = false;
				}
			} catch (err) {
				console.error('Zap error:', err);
				toastError('Zap failed', (err as Error).message);
				isZapping = false;
			}
		} catch (err) {
			console.error('Zap error:', err);
			toastError('Zap failed', (err as Error).message);
			isZapping = false;
		}
	}

	function resetInvoice() {
		invoice = null;
	}

	return {
		...zapQuery,
		zap,
		get isZapping() {
			return isZapping;
		},
		get invoice() {
			return invoice;
		},
		set invoice(value: string | null) {
			invoice = value;
		},
		resetInvoice
	};
}
