<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { useDVMs } from '$lib/hooks/useDVMs.svelte';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import * as Card from '$lib/components/ui/card';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Loader2, Zap, ExternalLink, Globe, Smartphone, Monitor, ArrowRight, RefreshCw } from 'lucide-svelte';
	import { nip19 } from 'nostr-tools';

	// Map kind numbers to human-readable names
	const kindNames: Record<number, string> = {
		5000: 'Text Processing',
		5001: 'Text-to-Speech',
		5002: 'Speech-to-Text',
		5003: 'Translation',
		5004: 'Summarization',
		5005: 'Translation',
		5006: 'Text Extraction',
		5050: 'Search',
		5100: 'Geohashing',
		5200: 'Content Discovery',
		5250: 'User Discovery',
		5300: 'Content Discovery',
		5301: 'Geohashing',
		5302: 'Discovery'
	};

	const platformIcons: Record<string, typeof Globe> = {
		web: Globe,
		ios: Smartphone,
		android: Smartphone,
		desktop: Monitor
	};

	const { dvmServices } = useDVMs();

	// Debug logging
	$effect(() => {
		console.log('[DVM Page] DVMs count:', $dvmServices.length);
	});

	let selectedCategory = $state('all');

	// Get unique categories from DVMs
	const categories = $derived(['all', ...new Set($dvmServices.flatMap((dvm) => dvm.tags))]);

	// Filter DVMs by category
	let filteredDVMs = $derived(
		selectedCategory === 'all'
			? $dvmServices
			: $dvmServices.filter((dvm) => dvm.tags.includes(selectedCategory))
	);

	// Sort to show feed-capable DVMs first (those with kinds 5050, 5200, 5250, 5300)
	const sortedDVMs = $derived(
		filteredDVMs.sort((a, b) => {
			const aHasFeed = a.supportedKinds.some((k) => [5050, 5200, 5250, 5300].includes(k));
			const bHasFeed = b.supportedKinds.some((k) => [5050, 5200, 5250, 5300].includes(k));

			if (aHasFeed && !bHasFeed) return -1;
			if (!aHasFeed && bHasFeed) return 1;
			return 0;
		})
	);
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
				<div class="p-4">
					<h1 class="text-xl font-bold flex items-center gap-2 mb-2">
						<Zap class="h-5 w-5 text-blue-500" />
						Data Vending Machines
					</h1>
					<p class="text-sm text-muted-foreground mb-2">
						AI-powered services that curate and process Nostr content
					</p>
					<div class="bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg p-3 mb-3">
						<p class="text-xs text-blue-800 dark:text-blue-200">
							<strong>Experimental:</strong> DVM feeds are a new feature. Many listed DVMs may not be operational.
							DVMs process requests asynchronously and responses may take time.
						</p>
					</div>

					<!-- Category filters -->
					<div class="flex gap-2 overflow-x-auto pb-2 -mb-2">
						{#each categories as category}
							<Button
								variant={selectedCategory === category ? 'default' : 'outline'}
								size="sm"
								onclick={() => (selectedCategory = category)}
								class="flex-shrink-0"
							>
								{category}
							</Button>
						{/each}
					</div>
				</div>
			</div>

			<!-- Content -->
			{#if sortedDVMs.length === 0}
				<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
					<Zap class="h-16 w-16 text-muted-foreground mb-4" />
					<h2 class="text-2xl font-bold mb-2">No DVMs Found</h2>
					<p class="text-muted-foreground max-w-sm mb-4">
						{selectedCategory === 'all'
							? 'No Data Vending Machines are currently available. This might be because DVMs are not announced on your configured relays.'
							: `No DVMs found for category "${selectedCategory}"`}
					</p>
					<p class="text-xs text-muted-foreground">
						DVMs announce themselves using kind 31990 events. Try configuring different relays in Settings or check back later.
					</p>
				</div>
			{:else}
				<div class="p-4 space-y-4">
					{#each sortedDVMs as dvm (dvm.id)}
						{@const npub = nip19.npubEncode(dvm.pubkey)}

						<Card.Root class="hover:shadow-md transition-shadow">
							<Card.Header>
								<div class="flex items-start gap-3">
									<Avatar class="w-12 h-12">
										<AvatarImage src={dvm.picture} alt={dvm.name || 'DVM'} />
										<AvatarFallback>
											<Zap class="h-6 w-6" />
										</AvatarFallback>
									</Avatar>
									<div class="flex-1 min-w-0">
										<Card.Title class="text-lg">
											{dvm.name || 'Unnamed DVM'}
										</Card.Title>
										<Card.Description class="text-sm text-muted-foreground mt-1">
											{npub.slice(0, 16)}...
										</Card.Description>
									</div>
								</div>
							</Card.Header>
							<Card.Content class="space-y-3">
								{#if dvm.about}
									<p class="text-sm text-muted-foreground">
										{dvm.about}
									</p>
								{/if}

								<!-- Supported Kinds -->
								{#if dvm.supportedKinds.length > 0}
									<div>
										<h4 class="text-xs font-semibold text-muted-foreground uppercase mb-2">
											Supported Services
										</h4>
										<div class="flex flex-wrap gap-2">
											{#each dvm.supportedKinds as kind}
												<Badge variant="secondary">
													{kindNames[kind] || `Kind ${kind}`}
												</Badge>
											{/each}
										</div>
									</div>
								{/if}

								<!-- Tags -->
								{#if dvm.tags.length > 0}
									<div>
										<h4 class="text-xs font-semibold text-muted-foreground uppercase mb-2">
											Topics
										</h4>
										<div class="flex flex-wrap gap-2">
											{#each dvm.tags as tag}
												<Badge variant="outline">
													#{tag}
												</Badge>
											{/each}
										</div>
									</div>
								{/if}

								<!-- Handlers/Platforms -->
								{#if dvm.handlers.length > 0}
									<div>
										<h4 class="text-xs font-semibold text-muted-foreground uppercase mb-2">
											Available On
										</h4>
										<div class="flex flex-wrap gap-2">
											{#each dvm.handlers as handler, idx}
												{@const Icon = platformIcons[handler.platform] || Globe}
												<Button
													variant="outline"
													size="sm"
													class="gap-2"
													onclick={() => {
														console.log('Open DVM handler:', handler.url);
													}}
												>
													<Icon class="h-4 w-4" />
													{handler.platform}
													<ExternalLink class="h-3 w-3" />
												</Button>
											{/each}
										</div>
									</div>
								{/if}

								<!-- View Feed Button -->
								{#if dvm.supportedKinds.some((k) => [5050, 5200, 5250, 5300].includes(k))}
									<div class="pt-3 border-t">
										<a href="/dvm/{dvm.id}">
											<Button class="w-full gap-2" variant="default">
												<Zap class="h-4 w-4" />
												View Feed
												<ArrowRight class="h-4 w-4 ml-auto" />
											</Button>
										</a>
									</div>
								{/if}
							</Card.Content>
						</Card.Root>
					{/each}
				</div>
			{/if}
		</div>
	{/snippet}
</MainLayout>
