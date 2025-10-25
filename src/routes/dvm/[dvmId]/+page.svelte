<script lang="ts">
	import { page } from '$app/stores';
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { useDVMs } from '$lib/hooks/useDVMs.svelte';
	import { useDVMFeedReactive } from '$lib/hooks/useDVMFeed.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { Loader2, Zap, RefreshCw, ArrowLeft } from 'lucide-svelte';
	import Note from '$lib/components/Note.svelte';

	const kindNames: Record<number, string> = {
		5050: 'Search',
		5200: 'Content Discovery',
		5250: 'User Discovery',
		5300: 'Content Discovery'
	};

	const dvmId = $derived($page.params.dvmId);
	const { dvmServices } = useDVMs();

	// Find the DVM
	const dvm = $derived($dvmServices.find((d) => d.id === dvmId || d.pubkey === dvmId));

	// Get the first supported kind for content discovery (5300 is most common)
	const requestKind = $derived(
		dvm?.supportedKinds.find((k) => [5300, 5200, 5050, 5250].includes(k)) || 5300
	);

	// Create reactive DVM feed using welshman's FeedController
	let dvmFeed = $state<ReturnType<typeof useDVMFeedReactive> | null>(null);

	// Initialize feed when DVM and user are available (only once)
	let feedInitialized = $state(false);

	$effect(() => {
		if (dvm && $currentUser && !feedInitialized) {
			feedInitialized = true;
			try {
				dvmFeed = useDVMFeedReactive(dvm.pubkey, requestKind, {
					limit: 50,
					params: {}
				});
			} catch (error) {
				console.error('Failed to initialize DVM feed:', error);
				feedInitialized = false; // Allow retry
			}
		} else if (!dvm || !$currentUser) {
			dvmFeed = null;
			feedInitialized = false;
		}
	});

	// Track if we've attempted to load
	let loadAttempted = $state(false);

	// Auto-start DVM feed when initialized
	$effect(() => {
		if (dvmFeed && !dvmFeed.isLoading && dvmFeed.events.length === 0 && !loadAttempted) {
			loadAttempted = true;
			dvmFeed.load(50);
		}
	});

	function handleRequestFeed() {
		if (dvmFeed) {
			dvmFeed.load(50);
		}
	}

	function handleRefresh() {
		if (dvmFeed) {
			dvmFeed.load(50);
		}
	}
</script>

{#if !dvm && $dvmServices.length === 0}
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
				<p class="ml-3 text-muted-foreground">Loading DVMs...</p>
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
								disabled={!$currentUser || dvmFeed?.isLoading}
								class="gap-2"
								size="sm"
							>
								{#if dvmFeed?.isLoading}
									<Loader2 class="h-4 w-4 animate-spin" />
									Loading...
								{:else}
									<Zap class="h-4 w-4" />
									Request Feed
								{/if}
							</Button>
							<Button
								onclick={handleRefresh}
								disabled={dvmFeed?.isLoading}
								variant="outline"
								size="sm"
								class="gap-2"
							>
								<RefreshCw class={`h-4 w-4 ${dvmFeed?.isLoading ? 'animate-spin' : ''}`} />
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
				{:else if dvmFeed?.isLoading && dvmFeed.events.length === 0}
					<div class="flex items-center justify-center py-20">
						<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
					</div>
				{:else if dvmFeed?.error}
					<div class="p-8 text-center">
						<Card.Root>
							<Card.Content class="py-12">
								<Zap class="h-12 w-12 text-destructive mx-auto mb-4" />
								<h3 class="text-lg font-semibold mb-2">Error Loading Feed</h3>
								<p class="text-muted-foreground mb-4">
									{dvmFeed.error.message}
								</p>
								<Button onclick={handleRequestFeed}>
									<RefreshCw class="h-4 w-4 mr-2" />
									Try Again
								</Button>
							</Card.Content>
						</Card.Root>
					</div>
				{:else if !dvmFeed || dvmFeed.events.length === 0}
					<div class="p-8 text-center">
						<Card.Root>
							<Card.Content class="py-12">
								<Zap class="h-12 w-12 text-muted-foreground mx-auto mb-4" />
								<h3 class="text-lg font-semibold mb-2">DVM Feeds - Experimental Feature</h3>
								<p class="text-muted-foreground mb-4 max-w-md mx-auto">
									Data Vending Machines (DVMs) are AI-powered services that curate content for you.
									This feature is currently experimental.
								</p>
								<div class="bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800 rounded-lg p-4 mb-4 max-w-md mx-auto">
									<p class="text-sm text-yellow-800 dark:text-yellow-200">
										<strong>Note:</strong> DVMs process requests asynchronously and may take time to respond.
										The DVM at pubkey <code class="text-xs bg-yellow-100 dark:bg-yellow-900/40 px-1 py-0.5 rounded">{dvm.pubkey.slice(0, 16)}...</code>
										{dvmFeed?.isExhausted ? 'did not return any results' : 'has not responded yet'}.
									</p>
								</div>
								<p class="text-xs text-muted-foreground mb-4">
									DVMs need to be online and actively processing requests. Many DVMs listed may not be operational.
								</p>
								<Button onclick={() => (window.location.href = '/dvm')} variant="outline">
									<ArrowLeft class="h-4 w-4 mr-2" />
									Back to DVM List
								</Button>
							</Card.Content>
						</Card.Root>
					</div>
				{:else}
					<div>
						{#each dvmFeed.events as event (event.id)}
							<Note {event} />
						{/each}
						{#if dvmFeed.isExhausted}
							<div class="p-4 text-center text-sm text-muted-foreground">
								No more results available
							</div>
						{/if}
					</div>
				{/if}
			</div>
		{/snippet}
	</MainLayout>
{/if}
