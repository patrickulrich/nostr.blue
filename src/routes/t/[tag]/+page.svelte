<script lang="ts">
	import { page } from '$app/stores';
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import Note from '$lib/components/Note.svelte';
	import { createInfiniteQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import { Button } from '$lib/components/ui/button';
	import { Loader2, RefreshCw, Hash } from 'lucide-svelte';
	import { onMount } from 'svelte';

	let hashtag = $derived($page.params.tag);

	const hashtagQuery = createInfiniteQuery(() => ({
		queryKey: ['hashtag-feed', hashtag],
		queryFn: async ({ pageParam, signal }) => {
			if (!hashtag) return [];

			try {
				const events = await loadWithRouter({
					filters: [
						{
							kinds: [1],
							'#t': [hashtag.toLowerCase()],
							limit: 20,
							...(pageParam ? { until: pageParam } : {})
						}
					],
					signal,
				});

				return events.sort((a, b) => b.created_at - a.created_at);
			} catch (error) {
				console.error('Hashtag feed query error:', error);
				return [];
			}
		},
		getNextPageParam: (lastPage) => {
			if (!lastPage || lastPage.length === 0) {
				return null;
			}
			const oldestEvent = lastPage[lastPage.length - 1];
			return oldestEvent ? oldestEvent.created_at - 1 : null;
		},
		initialPageParam: null as number | null,
		enabled: !!hashtag,
		staleTime: 30000
	}));

	let allEvents = $derived(hashtagQuery.data?.pages.flatMap((page) => page) || []);

	// Infinite scroll observer
	let observerTarget: HTMLDivElement | null = $state(null);

	onMount(() => {
		if (!hashtag) return;

		const observer = new IntersectionObserver(
			(entries) => {
				if (
					entries[0]?.isIntersecting &&
					hashtagQuery.hasNextPage &&
					!hashtagQuery.isFetchingNextPage
				) {
					hashtagQuery.fetchNextPage();
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

<svelte:head>
	<title>{hashtag ? `#${hashtag}` : 'Hashtag'} / nostr.blue</title>
	<meta
		name="description"
		content={hashtag ? `Posts tagged with #${hashtag} on Nostr` : 'Browse posts by hashtag'}
	/>
</svelte:head>

<MainLayout>
	{#snippet sidebar()}
		<AppSidebar />
	{/snippet}

	{#snippet rightPanel()}
		<RightSidebar />
	{/snippet}

	{#snippet children()}
		{#if !hashtag}
			<div class="flex flex-col items-center justify-center py-20 px-4 text-center min-h-screen">
				<Hash class="h-16 w-16 text-muted-foreground mb-4" />
				<h2 class="text-2xl font-bold mb-2">Invalid hashtag</h2>
				<p class="text-muted-foreground max-w-sm">
					Please provide a valid hashtag to search for.
				</p>
			</div>
		{:else}
			<div class="min-h-screen">
				<!-- Header -->
				<div class="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
					<div class="flex items-center justify-between p-4">
						<div class="flex items-center gap-3">
							<Hash class="h-6 w-6 text-blue-500" />
							<div>
								<h1 class="text-xl font-bold">#{hashtag}</h1>
								<p class="text-sm text-muted-foreground">Posts tagged with this hashtag</p>
							</div>
						</div>
						<Button
							variant="ghost"
							size="icon"
							onclick={() => hashtagQuery.refetch()}
							disabled={hashtagQuery.isRefetching}
						>
							<RefreshCw class={`h-5 w-5 ${hashtagQuery.isRefetching ? 'animate-spin' : ''}`} />
						</Button>
					</div>
				</div>

				<!-- Feed -->
				{#if hashtagQuery.isLoading}
					<div class="flex items-center justify-center py-20">
						<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
					</div>
				{:else if allEvents.length === 0}
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<Hash class="h-16 w-16 text-muted-foreground mb-4" />
						<h2 class="text-2xl font-bold mb-2">No posts found</h2>
						<p class="text-muted-foreground max-w-sm">
							There are no posts with #{hashtag} yet. Be the first to post about this topic!
						</p>
					</div>
				{:else}
					{#each allEvents as event (event.id)}
						<Note {event} />
					{/each}

					<!-- Infinite scroll trigger -->
					<div bind:this={observerTarget} class="py-8 flex justify-center">
						{#if hashtagQuery.isFetchingNextPage}
							<Loader2 class="h-6 w-6 animate-spin text-blue-500" />
						{:else if !hashtagQuery.hasNextPage && allEvents.length > 0}
							<p class="text-muted-foreground text-sm">You've reached the end</p>
						{/if}
					</div>
				{/if}
			</div>
		{/if}
	{/snippet}
</MainLayout>
