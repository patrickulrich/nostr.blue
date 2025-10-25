<script lang="ts">
	import { useTrendingNotes } from '$lib/hooks/useTrendingNotes.svelte';
	import Note from '$lib/components/Note.svelte';
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import TrendingRightSidebar from '$lib/components/TrendingRightSidebar.svelte';
	import { TrendingUp } from 'lucide-svelte';

	const trendingQuery = useTrendingNotes();
	let trendingNotes = $derived(trendingQuery.data || []);
	let isLoading = $derived(trendingQuery.isLoading);
</script>

<svelte:head>
	<title>Trending / nostr.blue</title>
	<meta name="description" content="Discover trending posts on Nostr" />
</svelte:head>

<MainLayout>
	{#snippet sidebar()}
		<AppSidebar />
	{/snippet}

	{#snippet rightPanel()}
		<TrendingRightSidebar />
	{/snippet}

	{#snippet children()}
		<div class="min-h-screen">
			<!-- Header -->
			<div class="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
				<div class="flex items-center gap-4 p-4">
					<TrendingUp class="h-6 w-6" />
					<div>
						<h1 class="text-xl font-bold">Trending</h1>
						<p class="text-sm text-muted-foreground">
							Posts trending on Nostr in the last 24 hours
						</p>
					</div>
				</div>
			</div>

			<!-- Content -->
			{#if isLoading}
				<div class="flex items-center justify-center py-20">
					<svg
						class="h-8 w-8 animate-spin text-blue-500"
						xmlns="http://www.w3.org/2000/svg"
						fill="none"
						viewBox="0 0 24 24"
					>
						<circle
							class="opacity-25"
							cx="12"
							cy="12"
							r="10"
							stroke="currentColor"
							stroke-width="4"
						></circle>
						<path
							class="opacity-75"
							fill="currentColor"
							d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"
						></path>
					</svg>
				</div>
			{:else if trendingNotes.length === 0}
				<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
					<TrendingUp class="h-16 w-16 text-muted-foreground mb-4" />
					<h2 class="text-2xl font-bold mb-2">No trending posts</h2>
					<p class="text-muted-foreground max-w-sm">
						Check back later to see what's trending on Nostr.
					</p>
				</div>
			{:else}
				<div class="border-b border-border p-4 bg-muted/30">
					<p class="text-sm text-muted-foreground">
						Showing {trendingNotes.length} trending {trendingNotes.length === 1
							? 'post'
							: 'posts'} from
						<a
							href="https://nostr.band"
							target="_blank"
							rel="noopener noreferrer"
							class="text-blue-500 hover:underline"
						>
							Nostr.Band
						</a>
					</p>
				</div>
				{#each trendingNotes as note (note.event.id)}
					<Note event={note.event} />
				{/each}
			{/if}
		</div>
	{/snippet}
</MainLayout>
