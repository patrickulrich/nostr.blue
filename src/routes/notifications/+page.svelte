<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import NotificationItem from '$lib/components/NotificationItem.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Loader2, RefreshCw, Bell } from 'lucide-svelte';
	import { currentUser } from '$lib/stores/auth';
	import { createInfiniteQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import { categorizeNotifications, type NotificationEvent, type NotificationType } from '$lib/stores/notifications.svelte';
	import { onMount } from 'svelte';
	import { cn } from '$lib/utils';

	// Notification filter state
	type NotificationFilter = 'all' | NotificationType;
	let notificationFilter = $state<NotificationFilter>('all');

	// Fetch notifications with infinite scroll
	const notificationsQuery = createInfiniteQuery<NotificationEvent[]>(() => ({
		queryKey: ['notifications', $currentUser?.pubkey],
		queryFn: async ({ pageParam, signal }) => {
			if (!$currentUser?.pubkey) return [];

			const limit = 50;
			const filters = {
				'#p': [$currentUser.pubkey],
				limit,
				...(pageParam ? { until: pageParam as number } : {})
			};

			// Query for different notification kinds
			const kinds = [1, 7, 6, 9735] as const; // text notes, reactions, reposts, zaps
			const filterArray = kinds.map((k) => ({ kinds: [k], ...filters }));

			const events = await loadWithRouter({
				filters: filterArray,
				signal,
			});

			// Categorize and sort notifications
			const notifications = categorizeNotifications(events);

			return notifications.slice(0, limit);
		},
		getNextPageParam: (lastPage) => {
			if (!lastPage || lastPage.length === 0) {
				return undefined;
			}

			const oldestNotification = lastPage[lastPage.length - 1];
			return oldestNotification ? oldestNotification.event.created_at - 1 : undefined;
		},
		initialPageParam: undefined,
		enabled: !!$currentUser?.pubkey,
		staleTime: 30000 // 30 seconds
	}));

	let allNotifications = $derived(
		notificationsQuery.data?.pages.flatMap((page) => page) as NotificationEvent[] | undefined
	);

	// Filter notifications based on selected filter
	let filteredNotifications = $derived.by(() => {
		if (!allNotifications) return undefined;
		if (notificationFilter === 'all') return allNotifications;
		return allNotifications.filter((n) => n.type === notificationFilter);
	});

	let isLoading = $derived(notificationsQuery.isLoading);
	let isRefetching = $derived(notificationsQuery.isRefetching);

	// Intersection observer for infinite scroll
	let observerTarget: HTMLDivElement | null = $state(null);

	onMount(() => {
		const observer = new IntersectionObserver(
			(entries) => {
				if (
					entries[0]?.isIntersecting &&
					notificationsQuery.hasNextPage &&
					!notificationsQuery.isFetchingNextPage
				) {
					notificationsQuery.fetchNextPage();
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
				<div class="flex items-center justify-between px-4 pt-4">
					<h1 class="text-xl font-bold">Notifications</h1>
					{#if $currentUser}
						<Button
							variant="ghost"
							size="icon"
							onclick={() => notificationsQuery.refetch()}
							disabled={isRefetching}
							class="flex-shrink-0"
						>
							<RefreshCw class={`h-5 w-5 ${isRefetching ? 'animate-spin' : ''}`} />
						</Button>
					{/if}
				</div>

				<!-- Filter Tabs -->
				{#if $currentUser}
					<div class="flex items-center gap-1 px-4 pt-2 overflow-x-auto">
						<button
							onclick={() => (notificationFilter = 'all')}
							class={cn(
								'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
								notificationFilter === 'all' ? 'text-foreground' : 'text-muted-foreground'
							)}
						>
							All
							{#if notificationFilter === 'all'}
								<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
							{/if}
						</button>
						<button
							onclick={() => (notificationFilter = 'reply')}
							class={cn(
								'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
								notificationFilter === 'reply' ? 'text-foreground' : 'text-muted-foreground'
							)}
						>
							Replies
							{#if notificationFilter === 'reply'}
								<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
							{/if}
						</button>
						<button
							onclick={() => (notificationFilter = 'mention')}
							class={cn(
								'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
								notificationFilter === 'mention' ? 'text-foreground' : 'text-muted-foreground'
							)}
						>
							Mentions
							{#if notificationFilter === 'mention'}
								<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
							{/if}
						</button>
						<button
							onclick={() => (notificationFilter = 'reaction')}
							class={cn(
								'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
								notificationFilter === 'reaction' ? 'text-foreground' : 'text-muted-foreground'
							)}
						>
							Reactions
							{#if notificationFilter === 'reaction'}
								<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
							{/if}
						</button>
						<button
							onclick={() => (notificationFilter = 'repost')}
							class={cn(
								'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
								notificationFilter === 'repost' ? 'text-foreground' : 'text-muted-foreground'
							)}
						>
							Reposts
							{#if notificationFilter === 'repost'}
								<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
							{/if}
						</button>
						<button
							onclick={() => (notificationFilter = 'zap')}
							class={cn(
								'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
								notificationFilter === 'zap' ? 'text-foreground' : 'text-muted-foreground'
							)}
						>
							Zaps
							{#if notificationFilter === 'zap'}
								<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
							{/if}
						</button>
					</div>
				{/if}
			</div>

			<!-- Content -->
			{#if !$currentUser}
				<!-- Login prompt -->
				<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
					<Bell class="h-16 w-16 text-muted-foreground mb-4" />
					<h2 class="text-2xl font-bold mb-2">Sign in to see your notifications</h2>
					<p class="text-muted-foreground max-w-sm">
						Connect your Nostr account to view replies, mentions, reactions, and more.
					</p>
				</div>
			{:else if isLoading}
				<!-- Loading state -->
				<div class="flex items-center justify-center py-20">
					<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
				</div>
			{:else if !filteredNotifications || filteredNotifications.length === 0}
				<!-- Empty state -->
				<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
					<Bell class="h-16 w-16 text-muted-foreground mb-4" />
					<h2 class="text-2xl font-bold mb-2">
						{#if notificationFilter === 'all'}
							No notifications yet
						{:else}
							No {notificationFilter}s yet
						{/if}
					</h2>
					<p class="text-muted-foreground max-w-sm">
						{#if notificationFilter === 'all'}
							When someone interacts with your posts, you'll see it here.
						{:else}
							When someone {notificationFilter}s your posts, you'll see it here.
						{/if}
					</p>
				</div>
			{:else}
				<!-- Notifications list -->
				{#each filteredNotifications as notification, index (`${notification.event.id}-${index}`)}
					<NotificationItem {notification} />
				{/each}

				<!-- Infinite scroll trigger -->
				<div bind:this={observerTarget} class="py-8 flex justify-center">
					{#if notificationsQuery.isFetchingNextPage}
						<Loader2 class="h-6 w-6 animate-spin text-blue-500" />
					{:else if !notificationsQuery.hasNextPage && filteredNotifications && filteredNotifications.length > 0}
						<p class="text-muted-foreground text-sm">You've reached the end</p>
					{/if}
				</div>
			{/if}
		</div>
	{/snippet}
</MainLayout>
