<script lang="ts">
	import { Search, TrendingUp, Heart, MessageCircle, Loader2 } from 'lucide-svelte';
	import { Input } from '$lib/components/ui/input';
	import * as Card from '$lib/components/ui/card';
	import { goto } from '$app/navigation';
	import { useTrendingNotes } from '$lib/hooks/useTrendingNotes.svelte';
	import { nip19 } from 'nostr-tools';

	let searchQuery = $state('');

	function handleSearch(e: SubmitEvent) {
		e.preventDefault();
		if (searchQuery.trim()) {
			goto(`/search?q=${encodeURIComponent(searchQuery)}`);
		}
	}

	// Fetch trending notes from Nostr.Band
	const trendingQuery = useTrendingNotes(10);

	// Derived reactive values from query
	let trendingData = $derived(trendingQuery.data);
	let trendingLoading = $derived(trendingQuery.isLoading);
	let trendingError = $derived(trendingQuery.isError);

	function getDisplayName(note: any): string {
		const npub = nip19.npubEncode(note.event.pubkey);
		return note.profile?.display_name || note.profile?.name || npub.slice(0, 12);
	}

	function truncateContent(content: string, maxLength: number = 100): string {
		if (content.length <= maxLength) return content;
		return content.slice(0, maxLength).trim() + '...';
	}
</script>

<div class="flex flex-col gap-4 sticky top-0 pt-2 pb-2 h-screen overflow-hidden">
	<!-- Search Bar -->
	<form onsubmit={handleSearch} class="relative flex-shrink-0">
		<Search
			class="absolute left-3 top-1/2 transform -translate-y-1/2 h-5 w-5 text-muted-foreground"
		/>
		<Input
			type="text"
			placeholder="Search"
			bind:value={searchQuery}
			class="pl-11 bg-muted/50 border-none rounded-full h-11 focus-visible:ring-1 focus-visible:ring-blue-500"
			aria-label="Search posts and users"
		/>
	</form>

	<!-- Nostr.Band Trending -->
	<Card.Root class="border-border flex-1 flex flex-col overflow-hidden">
		<Card.Header class="pb-3 flex-shrink-0">
			<Card.Title class="text-xl flex items-center gap-2">
				<TrendingUp class="h-5 w-5" />
				Nostr.Band Trending
			</Card.Title>
		</Card.Header>
		<Card.Content class="p-0 flex-1 flex flex-col overflow-hidden">
			{#if trendingLoading}
				<div class="flex items-center justify-center py-8">
					<Loader2 class="h-6 w-6 animate-spin text-blue-500" />
				</div>
			{:else if trendingError}
				<div class="px-4 py-8 text-center text-sm text-muted-foreground">
					Failed to load trending posts
				</div>
			{:else if !trendingData || trendingData.length === 0}
				<div class="px-4 py-8 text-center text-sm text-muted-foreground">
					No trending posts right now
				</div>
			{:else}
				<div class="flex-1 overflow-y-auto scrollbar-hide">
					{#each trendingData as note}
						{@const noteId = nip19.noteEncode(note.event.id)}
						{@const npub = nip19.npubEncode(note.event.pubkey)}
						{@const authorName = getDisplayName(note)}
						<a
							href="/{noteId}"
							class="block px-4 py-3 hover:bg-accent/50 transition-colors border-b border-border last:border-0"
						>
							<div class="flex gap-3">
								<img
									src={note.profile?.picture ||
										`https://api.dicebear.com/7.x/identicon/svg?seed=${note.event.pubkey}`}
									alt={authorName}
									class="w-10 h-10 rounded-full flex-shrink-0"
								/>
								<div class="flex-1 min-w-0">
									<div class="text-sm font-semibold truncate mb-1">{authorName}</div>
									<div class="text-sm mb-2 line-clamp-2">
										{truncateContent(note.event.content, 100)}
									</div>
									<div class="flex items-center gap-3 text-xs text-muted-foreground">
										{#if note.stats?.reactions && note.stats.reactions > 0}
											<span class="flex items-center gap-1">
												<Heart class="h-3 w-3" />
												{note.stats.reactions}
											</span>
										{/if}
										{#if note.stats?.replies && note.stats.replies > 0}
											<span class="flex items-center gap-1">
												<MessageCircle class="h-3 w-3" />
												{note.stats.replies}
											</span>
										{/if}
									</div>
								</div>
							</div>
						</a>
					{/each}
				</div>
				<button
					class="w-full px-4 py-3 text-blue-500 hover:bg-accent/50 transition-colors text-left text-sm flex-shrink-0 border-t border-border"
					onclick={() => goto('/trending')}
				>
					Show more
				</button>
			{/if}
		</Card.Content>
	</Card.Root>

	<!-- Footer Links -->
	<div class="px-4 text-xs text-muted-foreground flex flex-wrap gap-2 mt-auto flex-shrink-0">
		<a href="/terms" class="hover:underline">Terms of Service</a>
		<span>·</span>
		<a href="/privacy" class="hover:underline">Privacy Policy</a>
		<span>·</span>
		<a href="/cookies" class="hover:underline">Cookie Policy</a>
		<span>·</span>
		<a href="/about" class="hover:underline">About</a>
		<div class="w-full mt-1">© 2024 nostr.blue</div>
	</div>
</div>
