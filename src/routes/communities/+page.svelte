<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { useCommunities, useUserCommunities } from '$lib/hooks/useCommunities.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import Badge from '$lib/components/ui/Badge.svelte';
	import { Loader2, Users, Search, ArrowRight } from 'lucide-svelte';

	const communitiesQuery = useCommunities();
	const userCommunitiesQuery = useUserCommunities();

	let searchQuery = $state('');

	// Filter and sort communities
	let filteredCommunities = $derived.by(() => {
		if (!communitiesQuery.data) return [];

		const filtered = communitiesQuery.data.filter((community) => {
			if (!searchQuery) return true;

			const query = searchQuery.toLowerCase();
			return (
				community.name?.toLowerCase().includes(query) ||
				community.description?.toLowerCase().includes(query) ||
				community.dTag.toLowerCase().includes(query)
			);
		});

		// Sort user's communities first
		return filtered.sort((a, b) => {
			const aIsMember =
				userCommunitiesQuery.data?.has(a.aTag) ||
				($currentUser && a.moderators.includes($currentUser.pubkey));
			const bIsMember =
				userCommunitiesQuery.data?.has(b.aTag) ||
				($currentUser && b.moderators.includes($currentUser.pubkey));

			if (aIsMember && !bIsMember) return -1;
			if (!aIsMember && bIsMember) return 1;

			// Then sort by created date
			return b.event.created_at - a.event.created_at;
		});
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
				<div class="p-4">
					<h1 class="text-xl font-bold flex items-center gap-2 mb-3">
						<Users class="h-5 w-5 text-blue-500" />
						Communities
					</h1>
					<p class="text-sm text-muted-foreground mb-3">
						Discover communities and join the conversation
					</p>

					<!-- Search -->
					<div class="relative">
						<Search class="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
						<Input
							placeholder="Search communities..."
							bind:value={searchQuery}
							class="pl-10"
						/>
					</div>
				</div>
			</div>

			<!-- Content -->
			{#if communitiesQuery.isLoading}
				<div class="flex items-center justify-center py-20">
					<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
				</div>
			{:else if filteredCommunities.length === 0}
				<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
					<Users class="h-16 w-16 text-muted-foreground mb-4" />
					<h2 class="text-2xl font-bold mb-2">
						{searchQuery ? 'No communities found' : 'No communities available'}
					</h2>
					<p class="text-muted-foreground max-w-sm">
						{searchQuery
							? 'Try a different search term'
							: 'Connect to more relays to discover communities'}
					</p>
				</div>
			{:else}
				<div class="p-4 space-y-4">
					{#each filteredCommunities as community (community.id)}
						{@const isMember =
							userCommunitiesQuery.data?.has(community.aTag) ||
							($currentUser && community.moderators.includes($currentUser.pubkey))}
						{@const isModerator = $currentUser && community.moderators.includes($currentUser.pubkey)}

						<Card.Root class="hover:shadow-md transition-shadow">
							<Card.Header>
								<div class="flex items-start gap-3">
									<Avatar class="w-12 h-12">
										<AvatarImage src={community.image} alt={community.name || 'Community'} />
										<AvatarFallback>
											<Users class="h-6 w-6" />
										</AvatarFallback>
									</Avatar>
									<div class="flex-1 min-w-0">
										<div class="flex items-center gap-2">
											<Card.Title class="text-lg">
												{community.name || 'Unnamed Community'}
											</Card.Title>
											{#if isModerator}
												<Badge variant="default" class="text-xs">Moderator</Badge>
											{:else if isMember}
												<Badge variant="secondary" class="text-xs">Member</Badge>
											{/if}
										</div>
										<Card.Description class="text-sm text-muted-foreground mt-1">
											{community.dTag}
										</Card.Description>
									</div>
								</div>
							</Card.Header>
							<Card.Content class="space-y-3">
								{#if community.description}
									<p class="text-sm text-muted-foreground">
										{community.description}
									</p>
								{/if}

								<!-- Moderators -->
								{#if community.moderators.length > 0}
									<div>
										<h4 class="text-xs font-semibold text-muted-foreground uppercase mb-2">
											Moderators
										</h4>
										<p class="text-sm text-muted-foreground">
											{community.moderators.length} moderator{community.moderators.length !==
											1
												? 's'
												: ''}
										</p>
									</div>
								{/if}

								<!-- View Community Button -->
								<div class="pt-3 border-t">
									<a href="/community/{encodeURIComponent(community.aTag)}">
										<Button class="w-full gap-2" variant="default">
											<Users class="h-4 w-4" />
											View Community
											<ArrowRight class="h-4 w-4 ml-auto" />
										</Button>
									</a>
								</div>
							</Card.Content>
						</Card.Root>
					{/each}
				</div>
			{/if}
		</div>
	{/snippet}
</MainLayout>
