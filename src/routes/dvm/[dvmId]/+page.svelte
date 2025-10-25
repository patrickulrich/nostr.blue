<script lang="ts">
	import { page } from '$app/stores';
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { useDVMs } from '$lib/hooks/useDVMs.svelte';
	import { useDVMJob } from '$lib/hooks/useDVMJob.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { Loader2, Zap, RefreshCw, ArrowLeft } from 'lucide-svelte';
	import { createQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import Note from '$lib/components/Note.svelte';
	import type { TrustedEvent } from '@welshman/util';

	const kindNames: Record<number, string> = {
		5050: 'Search',
		5200: 'Content Discovery',
		5250: 'User Discovery',
		5300: 'Content Discovery'
	};

	const dvmId = $derived($page.params.dvmId);
	const { dvms, isLoading: loadingDVMs } = useDVMs();
	const { submitJob, useDVMFeed } = useDVMJob();

	let jobRequestId = $state<string | null>(null);
	let eventIds = $state<string[]>([]);
	let parsedDirectEvents = $state<TrustedEvent[]>([]);

	// Find the DVM
	const dvm = $derived(dvms.find((d) => d.id === dvmId || d.pubkey === dvmId));

	// Get the first supported kind for content discovery (5300 is most common)
	const requestKind = $derived(
		dvm?.supportedKinds.find((k) => [5300, 5200, 5050, 5250].includes(k)) || 5300
	);
	const resultKind = $derived(requestKind + 1000);

	// Create reactive values for the query
	const dvmPubkey = $derived(dvm?.pubkey || '');

	// Fetch the feed from this DVM using reactive query
	const feedQuery = $derived(useDVMFeed(dvmPubkey, requestKind, resultKind));

	async function handleRequestFeed() {
		if (!dvm || !$currentUser) return;

		try {
			const result = await submitJob.mutateAsync({
				kind: requestKind,
				targetPubkey: dvm.pubkey,
				params: {
					limit: '50'
				}
			});
			jobRequestId = result.id;
		} catch (error) {
			console.error('Failed to request feed:', error);
		}
	}

	// Auto-submit job request on mount if user is logged in
	$effect(() => {
		if (dvm && $currentUser && !jobRequestId) {
			handleRequestFeed();
		}
	});

	// Parse feed events - use only the most recent result for freshest recommendations
	$effect(() => {
		const ids: string[] = [];
		const directEvents: TrustedEvent[] = [];
		const feedEvents = feedQuery.data;

		// Use only the most recent DVM result (first in sorted array)
		const mostRecentResult = feedEvents?.[0];

		if (mostRecentResult) {
			try {
				const content = mostRecentResult.content.trim();

				// Try parsing as JSON first
				if (content.startsWith('[') || content.startsWith('{')) {
					const parsed = JSON.parse(content);

					if (Array.isArray(parsed)) {
						parsed.forEach((item) => {
							// Check if it's a tag array (DVM format: [["e", "id"], ["e", "id"]])
							if (Array.isArray(item) && item.length >= 2 && item[0] === 'e') {
								const eventId = item[1];
								if (typeof eventId === 'string' && eventId.length === 64) {
									ids.push(eventId);
								}
							}
							// Full event object
							else if (typeof item === 'object' && item.kind !== undefined) {
								directEvents.push(item as TrustedEvent);
							}
							// Event ID string
							else if (typeof item === 'string' && item.length === 64) {
								ids.push(item);
							}
						});
					} else if (typeof parsed === 'object' && parsed.kind !== undefined) {
						// Single event object
						directEvents.push(parsed as TrustedEvent);
					}
				} else {
					// Plain text - might be newline-separated event IDs
					const lines = content
						.split('\n')
						.map((l) => l.trim())
						.filter((l) => l.length === 64);
					ids.push(...lines);
				}
			} catch (e) {
				console.error('Failed to parse DVM result:', e, mostRecentResult.content);
			}
		}

		eventIds = ids;
		parsedDirectEvents = directEvents;
	});

	// Fetch full events if we have event IDs
	const eventsQuery = createQuery(() => ({
		queryKey: ['dvm-feed-events', eventIds],
		queryFn: async ({ signal }) => {
			if (eventIds.length === 0) return [];

			console.log('[DVMFeedPage] Querying relay for', eventIds.length, 'event IDs');

			try {
				const events = await loadWithRouter({
					filters: [{ ids: eventIds }],
					signal
				});

				console.log('[DVMFeedPage] Relay returned', events.length, 'events');

				// Sort by the DVM's ranking order (preserve order from eventIds)
				const eventMap = new Map(events.map((e) => [e.id, e]));
				return eventIds
					.map((id) => eventMap.get(id))
					.filter((e): e is TrustedEvent => e !== undefined);
			} catch (error) {
				console.error('Failed to fetch events by IDs:', error);
				return [];
			}
		},
		enabled: eventIds.length > 0,
		staleTime: 60000
	}));

	// Combine directly parsed events and fetched events
	const parsedEvents = $derived([...(parsedDirectEvents || []), ...(eventsQuery.data || [])]);
</script>

{#if loadingDVMs}
	<MainLayout>
		{#snippet sidebar()}
			<AppSidebar />
		{/snippet}

		{#snippet rightPanel()}
			<RightSidebar />
		{/snippet}

		{#snippet children()}
			<div class="flex items-center justify-center py-20">
				<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
			</div>
		{/snippet}
	</MainLayout>
{:else if !dvm}
	<MainLayout>
		{#snippet sidebar()}
			<AppSidebar />
		{/snippet}

		{#snippet rightPanel()}
			<RightSidebar />
		{/snippet}

		{#snippet children()}
			<div class="p-8 text-center">
				<h2 class="text-2xl font-bold mb-4">DVM Not Found</h2>
				<p class="text-muted-foreground mb-6">
					The requested Data Vending Machine could not be found.
				</p>
				<a href="/dvm">
					<Button>
						<ArrowLeft class="h-4 w-4 mr-2" />
						Back to DVMs
					</Button>
				</a>
			</div>
		{/snippet}
	</MainLayout>
{:else}
	<MainLayout>
		{#snippet sidebar()}
			<AppSidebar />
		{/snippet}

		{#snippet rightPanel()}
			<RightSidebar />
		{/snippet}

		{#snippet children()}
			<div class="min-h-screen">
				<!-- Header -->
				<div class="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
					<div class="p-4">
						<a
							href="/dvm"
							class="inline-flex items-center text-sm text-muted-foreground hover:text-foreground mb-3"
						>
							<ArrowLeft class="h-4 w-4 mr-1" />
							Back to DVMs
						</a>

						<div class="flex items-start gap-3 mb-3">
							<Avatar class="w-12 h-12">
								<AvatarImage src={dvm.picture} alt={dvm.name || 'DVM'} />
								<AvatarFallback>
									<Zap class="h-6 w-6" />
								</AvatarFallback>
							</Avatar>
							<div class="flex-1">
								<h1 class="text-xl font-bold">{dvm.name || 'Unnamed DVM'}</h1>
								{#if dvm.about}
									<p class="text-sm text-muted-foreground mt-1">{dvm.about}</p>
								{/if}
								<div class="flex flex-wrap gap-2 mt-2">
									{#each dvm.supportedKinds.filter((k) => [5050, 5200, 5250, 5300].includes(k)) as kind}
										<Badge variant="secondary" class="text-xs">
											{kindNames[kind] || `Kind ${kind}`}
										</Badge>
									{/each}
								</div>
							</div>
						</div>

						<div class="flex gap-2">
							<Button
								onclick={handleRequestFeed}
								disabled={!$currentUser || submitJob.isPending}
								class="gap-2"
								size="sm"
							>
								{#if submitJob.isPending}
									<Loader2 class="h-4 w-4 animate-spin" />
									Requesting...
								{:else}
									<Zap class="h-4 w-4" />
									Request Feed
								{/if}
							</Button>
							<Button
								onclick={() => feedQuery.refetch()}
								disabled={feedQuery.isLoading}
								variant="outline"
								size="sm"
								class="gap-2"
							>
								<RefreshCw class={`h-4 w-4 ${feedQuery.isLoading ? 'animate-spin' : ''}`} />
								Refresh
							</Button>
						</div>
					</div>
				</div>

				<!-- Feed Content -->
				{#if !$currentUser}
					<div class="p-8 text-center">
						<Card.Root>
							<Card.Content class="py-12">
								<Zap class="h-12 w-12 text-muted-foreground mx-auto mb-4" />
								<h3 class="text-lg font-semibold mb-2">Login Required</h3>
								<p class="text-muted-foreground">
									You need to be logged in to request feeds from DVMs.
								</p>
							</Card.Content>
						</Card.Root>
					</div>
				{:else if feedQuery.isLoading && parsedEvents.length === 0}
					<div class="flex items-center justify-center py-20">
						<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
					</div>
				{:else if parsedEvents.length === 0}
					<div class="p-8 text-center">
						<Card.Root>
							<Card.Content class="py-12">
								<Zap class="h-12 w-12 text-muted-foreground mx-auto mb-4" />
								<h3 class="text-lg font-semibold mb-2">No Results Yet</h3>
								<p class="text-muted-foreground mb-4">
									{feedQuery.data && feedQuery.data.length > 0
										? `Found ${feedQuery.data.length} DVM result(s) but couldn't parse them. Check console for details.`
										: 'No feed results have been published by this DVM yet. Try requesting a feed or check back later.'}
								</p>
								{#if feedQuery.data && feedQuery.data.length > 0}
									<p class="text-xs text-muted-foreground mb-4">
										Debug info: {eventIds.length} event IDs found, {parsedDirectEvents.length} direct
										events parsed. Open browser console (F12) to see raw DVM responses.
									</p>
								{/if}
								{#if !jobRequestId}
									<Button onclick={handleRequestFeed} disabled={submitJob.isPending}>
										<Zap class="h-4 w-4 mr-2" />
										Request Feed
									</Button>
								{/if}
							</Card.Content>
						</Card.Root>
					</div>
				{:else}
					<div>
						{#each parsedEvents as event (event.id)}
							<Note {event} />
						{/each}
					</div>
				{/if}
			</div>
		{/snippet}
	</MainLayout>
{/if}
