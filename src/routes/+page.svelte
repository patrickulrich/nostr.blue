<script lang="ts">
	import { createQuery, createInfiniteQuery } from '@tanstack/svelte-query';
	import type { TrustedEvent } from '@welshman/util';
	import { loadWithRouter } from '$lib/services/outbox';
	import Note from '$lib/components/Note.svelte';
	import NoteComposer from '$lib/components/NoteComposer.svelte';
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Loader2, RefreshCw } from 'lucide-svelte';
	import { currentUser } from '$lib/stores/auth';
	import { cn } from '$lib/utils';
	import { POPULAR_DVM_PUBKEY } from '$lib/stores/dvm.svelte';
	import { useDVMFeed } from '$lib/hooks/useDVMFeed.svelte';

	// Composer dialog state
	let composerOpen = $state(false);
	let replyToEventId = $state<string | undefined>(undefined);

	// Feed type state (Following/Popular)
	type FeedType = 'following' | 'popular';
	let feedType = $state<FeedType>($currentUser ? 'following' : 'popular');

	// Automatically switch to Following feed when user logs in
	$effect(() => {
		if ($currentUser && feedType === 'popular') {
			feedType = 'following';
		}
	});

	// Handle reply
	function handleReply(eventId: string) {
		replyToEventId = eventId;
		composerOpen = true;
	}

	// Handle composer close
	function handleComposerClose() {
		composerOpen = false;
		replyToEventId = undefined;
	}

	// Query user's contact list (following)
	const contactsQuery = createQuery<TrustedEvent | null>(() => ({
		queryKey: ['contacts', $currentUser?.pubkey],
		queryFn: async ({ signal }) => {
			if (!$currentUser?.pubkey) return null;

			const events = await loadWithRouter({
				filters: [
					{
						kinds: [3],
						authors: [$currentUser.pubkey],
						limit: 1
					}
				],
				signal
			});

			return events[0] || null;
		},
		enabled: !!$currentUser?.pubkey,
		staleTime: 60000 // Cache for 1 minute
	}));

	// Extract following list from contact event
	let following = $derived(
		contactsQuery.data?.tags
			.filter((tag) => tag[0] === 'p')
			.map((tag) => tag[1])
			.filter((pk): pk is string => typeof pk === 'string') || []
	);

	// Popular feed using DVM
	let popularEvents = $state<TrustedEvent[]>([]);
	let popularFeedController = $state<ReturnType<typeof useDVMFeed> | null>(null);
	let popularLoading = $state(false);
	let popularError = $state<Error | null>(null);
	let popularCleanup: (() => void) | null = null;

	// Initialize DVM feed controller for Popular feed
	$effect(() => {
		if (feedType === 'popular' && $currentUser && !popularFeedController) {
			try {
				popularFeedController = useDVMFeed(POPULAR_DVM_PUBKEY, 5300, {
					limit: 50,
					onEvent: (event) => {
						// Deduplicate and add to popularEvents
						if (!popularEvents.some((e) => e.id === event.id)) {
							popularEvents.push(event);
							// Sort by created_at descending
							popularEvents.sort((a, b) => b.created_at - a.created_at);
						}
						// Clear loading state when first event arrives
						if (popularLoading && popularEvents.length > 0) {
							popularLoading = false;
						}
					},
					onExhausted: () => {
						popularLoading = false;
					}
				});
			} catch (error) {
				console.error('Failed to initialize popular feed:', error);
				popularError = error instanceof Error ? error : new Error(String(error));
			}
		}

		// Cleanup when feed type changes away from popular
		return () => {
			if (popularCleanup) {
				popularCleanup();
				popularCleanup = null;
			}
		};
	});

	// Auto-start popular feed when controller is ready
	$effect(() => {
		if (
			feedType === 'popular' &&
			popularFeedController &&
			!popularLoading &&
			popularEvents.length === 0 &&
			!popularError &&
			!popularCleanup
		) {
			popularLoading = true;
			popularFeedController.listen(50)
				.then((cleanup) => {
					popularCleanup = cleanup;
				})
				.catch((err) => {
					popularError = err instanceof Error ? err : new Error(String(err));
					popularLoading = false;
				});
		}
	});

	// Query for Following feed with infinite scroll
	const followingFeedQuery = createInfiniteQuery<TrustedEvent[]>(() => {
		return {
			queryKey: ['following-feed', following],
			queryFn: async ({ pageParam, signal }) => {
				const limit = 20;

				// Following feed: Query from authors' write relays (outbox model)
				let events = await loadWithRouter({
					filters: [
						{
							kinds: [1], // Text notes
							authors: following,
							limit: limit * 4, // Fetch more since we filter out replies
							until: pageParam as number | undefined
						}
					],
					signal
				});

				// Filter out replies for cleaner feed
				events = events.filter(
					(event) => !event.tags.some((tag) => tag[0] === 'e')
				);

				// Sort by created_at descending (newest first)
				events = events.sort((a, b) => b.created_at - a.created_at);

				// Return up to limit events
				return events.slice(0, limit);
			},
			getNextPageParam: (lastPage) => {
				if (!lastPage || lastPage.length === 0) {
					return undefined;
				}

				const oldestEvent = lastPage[lastPage.length - 1];
				return oldestEvent ? oldestEvent.created_at - 1 : undefined;
			},
			initialPageParam: undefined,
			enabled: feedType === 'following' && following.length > 0,
			staleTime: 30000,
			retry: 2
		};
	});

	// Combined feed data based on feed type
	let feedData = $derived(
		feedType === 'following'
			? (followingFeedQuery.data?.pages.flatMap((page) => page) as TrustedEvent[] | undefined)
			: popularEvents
	);

	// Combined loading and error states
	let isLoading = $derived(
		feedType === 'following'
			? followingFeedQuery.isLoading
			: popularLoading
	);

	let error = $derived(
		feedType === 'following'
			? followingFeedQuery.error
			: popularError
	);

	let isRefetching = $derived(
		feedType === 'following'
			? followingFeedQuery.isRefetching
			: popularLoading
	);

	function refetchFeed() {
		if (feedType === 'following') {
			followingFeedQuery.refetch();
		} else if (popularFeedController) {
			// Stop existing listener
			if (popularCleanup) {
				popularCleanup();
				popularCleanup = null;
			}

			// Reset state
			popularLoading = true;
			popularEvents = [];
			popularError = null;

			// Restart listener
			popularFeedController.listen(50)
				.then((cleanup) => {
					popularCleanup = cleanup;
				})
				.catch((err) => {
					popularError = err instanceof Error ? err : new Error(String(err));
					popularLoading = false;
				});
		}
	}

	// Intersection observer for infinite scroll (Following feed only)
	let observerTarget: HTMLDivElement | null = $state(null);

	// Set up intersection observer that reacts to observerTarget changes
	$effect(() => {
		if (!observerTarget) return;

		const observer = new IntersectionObserver(
			(entries) => {
				if (
					entries[0]?.isIntersecting &&
					feedType === 'following' &&
					followingFeedQuery.hasNextPage &&
					!followingFeedQuery.isFetchingNextPage
				) {
					followingFeedQuery.fetchNextPage();
				}
			},
			{ threshold: 0.1 }
		);

		observer.observe(observerTarget);

		return () => {
			observer.disconnect();
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
			<div
				class="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border"
			>
				<div class="flex items-center justify-between px-4 pt-3">
					<div class="flex items-center gap-1 flex-1">
						<!-- Feed Selector Tabs -->
						{#if $currentUser}
							<button
								onclick={() => (feedType = 'following')}
								class={cn(
									'flex-1 px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative',
									feedType === 'following' ? 'text-foreground' : 'text-muted-foreground'
								)}
							>
								Following
								{#if feedType === 'following'}
									<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
								{/if}
							</button>
							<button
								onclick={() => (feedType = 'popular')}
								class={cn(
									'flex-1 px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative',
									feedType === 'popular' ? 'text-foreground' : 'text-muted-foreground'
								)}
							>
								Popular
								{#if feedType === 'popular'}
									<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
								{/if}
							</button>
						{:else}
							<h1 class="text-xl font-bold px-4 py-3">Home</h1>
						{/if}
					</div>
					<Button
						variant="ghost"
						size="icon"
						onclick={refetchFeed}
						disabled={isRefetching}
						class="flex-shrink-0"
					>
						<RefreshCw class={`h-5 w-5 ${isRefetching ? 'animate-spin' : ''}`} />
					</Button>
				</div>
			</div>

			<!-- Post Composer (for logged-in users) -->
			{#if $currentUser}
				<div class="border-b border-border">
					<div class="px-4 py-4">
						<button
							class="w-full text-left px-4 py-3 border rounded-lg hover:bg-accent transition-colors"
							onclick={() => (composerOpen = true)}
						>
							<span class="text-muted-foreground">What's happening?</span>
						</button>
					</div>
				</div>
			{/if}

			<!-- Note Composer Dialog -->
			<NoteComposer bind:isOpen={composerOpen} replyTo={replyToEventId} onClose={handleComposerClose} />

			<!-- Feed -->
			<div>
				{#if isLoading}
					<!-- Loading indicator -->
					<div class="flex items-center justify-center py-20">
						<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
					</div>
				{:else if error}
					<!-- Error state -->
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<p class="text-destructive mb-4">
							Failed to load feed: {error.message}
						</p>
						<Button onclick={refetchFeed}>
							Retry
						</Button>
					</div>
				{:else if feedData && feedData.length === 0}
					<!-- Empty state -->
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<div class="text-6xl mb-4">👋</div>
						{#if feedType === 'following' && following.length === 0}
							<h2 class="text-2xl font-bold mb-2">Follow some people!</h2>
							<p class="text-muted-foreground max-w-sm">
								You're not following anyone yet. Switch to the Popular feed to discover people to follow, or search for profiles to get started.
							</p>
						{:else}
							<h2 class="text-2xl font-bold mb-2">Welcome to nostr.blue!</h2>
							<p class="text-muted-foreground max-w-sm">
								Your decentralized social feed. Connect to relays and start following people to see their posts here.
							</p>
						{/if}
					</div>
				{:else if feedData}
					<!-- Notes list -->
					{#each feedData as event (event.id)}
						<Note {event} />
					{/each}

					<!-- Infinite scroll trigger -->
					{#if feedType === 'following'}
						<div bind:this={observerTarget} class="py-8 flex justify-center">
							{#if followingFeedQuery.isFetchingNextPage}
								<Loader2 class="h-6 w-6 animate-spin text-blue-500" />
							{:else if !followingFeedQuery.hasNextPage && feedData.length > 0}
								<p class="text-muted-foreground text-sm">
									You've reached the end
								</p>
							{/if}
						</div>
					{:else}
						<!-- Popular feed end message -->
						{#if feedData && feedData.length > 0}
							<div class="py-8 flex justify-center">
								<p class="text-muted-foreground text-sm">
									You've reached the end
								</p>
							</div>
						{/if}
					{/if}
				{/if}
			</div>
		</div>
	{/snippet}
</MainLayout>
