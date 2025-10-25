<script lang="ts">
	import { createInfiniteQuery } from '@tanstack/svelte-query';
	import type { TrustedEvent } from '@welshman/util';
	import { loadWithRouter } from '$lib/services/outbox';
	import Note from '$lib/components/Note.svelte';
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Loader2, RefreshCw } from 'lucide-svelte';
	import { cn } from '$lib/utils';
	import { onMount } from 'svelte';

	// Global feed query with infinite scroll
	const globalFeedQuery = createInfiniteQuery<TrustedEvent[]>(() => {
		return {
			queryKey: ['explore-feed'],
			queryFn: async ({ pageParam, signal }) => {
				const limit = 20;

				// Global feed: Query from default relays
				let events = await loadWithRouter({
					filters: [
						{
							kinds: [1], // Text notes
							limit: limit,
							until: pageParam as number | undefined
						}
					],
					signal
				});

				// Sort by created_at descending (newest first)
				events = events.sort((a, b) => b.created_at - a.created_at);

				return events;
			},
			getNextPageParam: (lastPage) => {
				if (!lastPage || lastPage.length === 0) {
					return undefined;
				}

				const oldestEvent = lastPage[lastPage.length - 1];
				return oldestEvent ? oldestEvent.created_at - 1 : undefined;
			},
			initialPageParam: undefined,
			staleTime: 30000,
			retry: 2
		};
	});

	// Flatten pages to get all events
	let feedData = $derived(
		globalFeedQuery.data?.pages.flatMap((page) => page) as TrustedEvent[] | undefined
	);

	// Intersection observer for infinite scroll
	let observerTarget: HTMLDivElement | null = $state(null);

	onMount(() => {
		const observer = new IntersectionObserver(
			(entries) => {
				if (
					entries[0]?.isIntersecting &&
					globalFeedQuery.hasNextPage &&
					!globalFeedQuery.isFetchingNextPage
				) {
					globalFeedQuery.fetchNextPage();
				}
			},
			{ threshold: 0.1 }
		);

		if (observerTarget) {
			observer.observe(observerTarget);
		}

		return () => {
			if (observerTarget) {
				observer.unobserve(observerTarget);
			}
		};
	});
</script>

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
				<div class="flex items-center justify-between px-4 pt-3">
					<div class="flex items-center gap-1 flex-1">
						<h1 class="text-xl font-bold px-4 py-3">Explore</h1>
					</div>
					<Button
						variant="ghost"
						size="icon"
						onclick={() => globalFeedQuery.refetch()}
						disabled={globalFeedQuery.isRefetching}
						class="flex-shrink-0"
					>
						<RefreshCw
							class={`h-5 w-5 ${globalFeedQuery.isRefetching ? 'animate-spin' : ''}`}
						/>
					</Button>
				</div>
			</div>

			<!-- Global Feed -->
			<div>
				{#if globalFeedQuery.isLoading}
					<!-- Loading indicator -->
					<div class="flex items-center justify-center py-20">
						<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
					</div>
				{:else if globalFeedQuery.error}
					<!-- Error state -->
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<p class="text-destructive mb-4">
							Failed to load feed: {globalFeedQuery.error.message}
						</p>
						<Button onclick={() => globalFeedQuery.refetch()}>
							Retry
						</Button>
					</div>
				{:else if feedData && feedData.length === 0}
					<!-- Empty state -->
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<div class="text-6xl mb-4">🔍</div>
						<h2 class="text-2xl font-bold mb-2">No posts found</h2>
						<p class="text-muted-foreground max-w-sm">
							Try checking your relay connections or come back later.
						</p>
					</div>
				{:else if feedData}
					<!-- Notes list -->
					{#each feedData as event (event.id)}
						<Note {event} />
					{/each}

					<!-- Infinite scroll trigger -->
					<div bind:this={observerTarget} class="py-8 flex justify-center">
						{#if globalFeedQuery.isFetchingNextPage}
							<Loader2 class="h-6 w-6 animate-spin text-blue-500" />
						{:else if !globalFeedQuery.hasNextPage && feedData.length > 0}
							<p class="text-muted-foreground text-sm">
								You've reached the end
							</p>
						{/if}
					</div>
				{/if}
			</div>
		</div>
	{/snippet}
</MainLayout>
