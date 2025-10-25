<script lang="ts">
	import { page } from '$app/stores';
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import Note from '$lib/components/Note.svelte';
	import { useCommunity, useCommunityPosts } from '$lib/hooks/useCommunities.svelte';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import { Button } from '$lib/components/ui/button';
	import { Loader2, Users, ArrowLeft, RefreshCw } from 'lucide-svelte';
	import Badge from '$lib/components/ui/Badge.svelte';

	let aTag = $derived(decodeURIComponent($page.params.aTag));

	const communityQuery = $derived(useCommunity(aTag));
	const postsQuery = $derived(useCommunityPosts(aTag));

	let community = $derived(communityQuery.data);
</script>

<svelte:head>
	<title>{community?.name || 'Community'} / nostr.blue</title>
	<meta name="description" content={community?.description || 'A Nostr community'} />
</svelte:head>

<MainLayout>
	{#snippet sidebar()}
		<AppSidebar />
	{/snippet}

	{#snippet rightPanel()}
		<RightSidebar />
	{/snippet}

	{#snippet children()}
		{#if communityQuery.isLoading}
			<div class="flex items-center justify-center py-20">
				<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
			</div>
		{:else if !community}
			<div class="p-8 text-center">
				<h2 class="text-2xl font-bold mb-4">Community Not Found</h2>
				<p class="text-muted-foreground mb-6">
					The requested community could not be found.
				</p>
				<a href="/communities">
					<Button>
						<ArrowLeft class="h-4 w-4 mr-2" />
						Back to Communities
					</Button>
				</a>
			</div>
		{:else}
			<div class="min-h-screen">
				<!-- Header -->
				<div class="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
					<div class="p-4">
						<a
							href="/communities"
							class="inline-flex items-center text-sm text-muted-foreground hover:text-foreground mb-3"
						>
							<ArrowLeft class="h-4 w-4 mr-1" />
							Back to Communities
						</a>

						<div class="flex items-start gap-3 mb-3">
							<Avatar class="w-12 h-12">
								<AvatarImage src={community.image} alt={community.name || 'Community'} />
								<AvatarFallback>
									<Users class="h-6 w-6" />
								</AvatarFallback>
							</Avatar>
							<div class="flex-1">
								<div class="flex items-center justify-between">
									<h1 class="text-xl font-bold">{community.name || 'Unnamed Community'}</h1>
									<Button
										onclick={() => postsQuery.refetch()}
										variant="ghost"
										size="icon"
									>
										<RefreshCw class="h-5 w-5" />
									</Button>
								</div>
								{#if community.description}
									<p class="text-sm text-muted-foreground mt-1">{community.description}</p>
								{/if}
								<div class="flex items-center gap-2 mt-2">
									<Badge variant="secondary" class="text-xs">
										{community.dTag}
									</Badge>
									{#if community.moderators.length > 0}
										<Badge variant="outline" class="text-xs">
											{community.moderators.length} moderator{community.moderators.length !== 1 ? 's' : ''}
										</Badge>
									{/if}
								</div>
							</div>
						</div>
					</div>
				</div>

				<!-- Posts -->
				{#if postsQuery.isLoading}
					<div class="flex items-center justify-center py-20">
						<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
					</div>
				{:else if postsQuery.data && postsQuery.data.length === 0}
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<Users class="h-16 w-16 text-muted-foreground mb-4" />
						<h2 class="text-2xl font-bold mb-2">No posts yet</h2>
						<p class="text-muted-foreground max-w-sm">
							This community doesn't have any posts yet. Be the first to post!
						</p>
					</div>
				{:else if postsQuery.data}
					<div>
						{#each postsQuery.data as event (event.id)}
							<Note {event} />
						{/each}

						{#if postsQuery.data.length > 0}
							<div class="py-8 flex justify-center">
								<p class="text-muted-foreground text-sm">
									You've reached the end
								</p>
							</div>
						{/if}
					</div>
				{/if}
			</div>
		{/if}
	{/snippet}
</MainLayout>
