<script lang="ts">
	import { Search, TrendingUp, Users, Loader2 } from 'lucide-svelte';
	import { Input } from '$lib/components/ui/input';
	import * as Card from '$lib/components/ui/card';
	import { goto } from '$app/navigation';
	import { useTrendingAuthors } from '$lib/hooks/useTrendingAuthors.svelte';
	import { nip19 } from 'nostr-tools';
	import { genUserName } from '$lib/genUserName';

	let searchQuery = $state('');

	function handleSearch(e: SubmitEvent) {
		e.preventDefault();
		if (searchQuery.trim()) {
			goto(`/search?q=${encodeURIComponent(searchQuery)}`);
		}
	}

	// Fetch trending authors from Nostr.Band
	const trendingQuery = useTrendingAuthors(15);

	// Derived reactive values from query
	let trendingData = $derived(trendingQuery.data);
	let trendingLoading = $derived(trendingQuery.isLoading);
	let trendingError = $derived(trendingQuery.isError);

	function getDisplayName(author: any): string {
		return author.profile?.display_name || author.profile?.name || genUserName(author.pubkey);
	}

	function truncateAbout(about: string | undefined, maxLength: number = 80): string {
		if (!about) return '';
		if (about.length <= maxLength) return about;
		return about.slice(0, maxLength).trim() + '...';
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

	<!-- Nostr.Band Trending Authors -->
	<Card.Root class="border-border flex-1 flex flex-col overflow-hidden">
		<Card.Header class="pb-3 flex-shrink-0">
			<Card.Title class="text-xl flex items-center gap-2">
				<Users class="h-5 w-5" />
				Trending Authors
			</Card.Title>
		</Card.Header>
		<Card.Content class="p-0 flex-1 flex flex-col overflow-hidden">
			{#if trendingLoading}
				<div class="flex items-center justify-center py-8">
					<Loader2 class="h-6 w-6 animate-spin text-blue-500" />
				</div>
			{:else if trendingError}
				<div class="px-4 py-8 text-center text-sm text-muted-foreground">
					Failed to load trending authors
				</div>
			{:else if !trendingData || trendingData.length === 0}
				<div class="px-4 py-8 text-center text-sm text-muted-foreground">
					No trending authors right now
				</div>
			{:else}
				<div class="flex-1 overflow-y-auto scrollbar-hide">
					{#each trendingData as author}
						{@const npub = nip19.npubEncode(author.pubkey)}
						{@const displayName = getDisplayName(author)}
						<a
							href="/{npub}"
							class="block px-4 py-3 hover:bg-accent/50 transition-colors border-b border-border last:border-0"
						>
							<div class="flex gap-3 items-start">
								<img
									src={author.profile?.picture ||
										`https://api.dicebear.com/7.x/identicon/svg?seed=${author.pubkey}`}
									alt={displayName}
									class="w-12 h-12 rounded-full flex-shrink-0"
								/>
								<div class="flex-1 min-w-0">
									<div class="flex items-center gap-1 mb-1">
										<span class="text-sm font-semibold truncate">{displayName}</span>
										{#if author.profile?.nip05}
											<svg
												class="h-4 w-4 text-blue-500 flex-shrink-0"
												fill="currentColor"
												viewBox="0 0 20 20"
											>
												<path
													fill-rule="evenodd"
													d="M6.267 3.455a3.066 3.066 0 001.745-.723 3.066 3.066 0 013.976 0 3.066 3.066 0 001.745.723 3.066 3.066 0 012.812 2.812c.051.643.304 1.254.723 1.745a3.066 3.066 0 010 3.976 3.066 3.066 0 00-.723 1.745 3.066 3.066 0 01-2.812 2.812 3.066 3.066 0 00-1.745.723 3.066 3.066 0 01-3.976 0 3.066 3.066 0 00-1.745-.723 3.066 3.066 0 01-2.812-2.812 3.066 3.066 0 00-.723-1.745 3.066 3.066 0 010-3.976 3.066 3.066 0 00.723-1.745 3.066 3.066 0 012.812-2.812zm7.44 5.252a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z"
													clip-rule="evenodd"
												/>
											</svg>
										{/if}
									</div>
									{#if author.profile?.about}
										<div class="text-xs text-muted-foreground mb-2">
											{truncateAbout(author.profile.about, 60)}
										</div>
									{/if}
									{#if author.stats?.followers_pubkey_count}
										<div class="flex items-center gap-1 text-xs text-muted-foreground">
											<Users class="h-3 w-3" />
											{author.stats.followers_pubkey_count.toLocaleString()} followers
										</div>
									{/if}
								</div>
							</div>
						</a>
					{/each}
				</div>
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
