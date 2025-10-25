<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { nip19 } from 'nostr-tools';
	import { createQuery, createInfiniteQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import type { TrustedEvent } from '@welshman/util';
	import Note from '$lib/components/Note.svelte';
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import * as Card from '$lib/components/ui/card';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import { genUserName } from '$lib/genUserName';
	import { Button } from '$lib/components/ui/button';
	import FollowButton from '$lib/components/FollowButton.svelte';
	import ReplyComposer from '$lib/components/ReplyComposer.svelte';
	import { useFollowing } from '$lib/hooks/useFollowing.svelte';
	import { useProfileStats } from '$lib/hooks/useProfileStats.svelte.ts';
	import { currentUser } from '$lib/stores/auth';
	import { ArrowLeft, Calendar, Settings2, Loader2 } from 'lucide-svelte';
	import { cn } from '$lib/utils';

	// Profile content filter type
	type ProfileFilter = 'posts' | 'replies' | 'articles' | 'media' | 'likes';
	let profileFilter = $state<ProfileFilter>('posts');

	// Define discriminated union types for the query data
	type ProfileData = {
		type: 'profile';
		pubkey: string;
		profile: Record<string, any>;
		profileEvent?: TrustedEvent;
	};

	type NoteData = {
		type: 'note';
		event: TrustedEvent | null;
	};

	type NIP19Data = ProfileData | NoteData | null;

	// Decode NIP-19 identifier
	let decoded = $derived.by(() => {
		try {
			return nip19.decode($page.params.nip19);
		} catch {
			// Return null for invalid identifiers - will show error state
			return null;
		}
	});

	let isInvalidIdentifier = $derived(!decoded);

	// Query based on decoded type
	const contentQuery = createQuery(() => {
		const nip19Param = $page.params.nip19;

		return {
			queryKey: ['nip19', nip19Param],
			queryFn: async () => {
				if (!decoded) return null;

				if (decoded.type === 'npub') {
					// Fetch profile for npub - router will use indexer relays
					const pubkey = decoded.data as string;
					const profiles = await loadWithRouter({
						filters: [
							{
								kinds: [0],
								authors: [pubkey],
								limit: 1
							}
						]
					});

					const profileEvent = profiles[0];
					const profile = profileEvent ? JSON.parse(profileEvent.content) : {};

					return {
						type: 'profile',
						pubkey,
						profile,
						profileEvent
					};
				} else if (decoded.type === 'note') {
					// Fetch single note - router will determine relays
					const noteId = decoded.data as string;
					const events = await loadWithRouter({
						filters: [
							{
								ids: [noteId]
							}
						]
					});

					return {
						type: 'note',
						event: events[0] || null
					};
				} else if (decoded.type === 'nevent') {
					// Fetch event with relay hints
					const data = decoded.data as { id: string; relays?: string[]; author?: string };
					const events = await loadWithRouter({
						filters: [
							{
								ids: [data.id]
							}
						],
						relayHints: data.relays // Pass relay hints from nevent
					});

					return {
						type: 'note',
						event: events[0] || null
					};
				} else if (decoded.type === 'nprofile') {
					// Fetch profile with relay hints
					const data = decoded.data as { pubkey: string; relays?: string[] };
					const profiles = await loadWithRouter({
						filters: [
							{
								kinds: [0],
								authors: [data.pubkey],
								limit: 1
							}
						],
						relayHints: data.relays
					});

					const profileEvent = profiles[0];
					const profile = profileEvent ? JSON.parse(profileEvent.content) : {};

					return {
						type: 'profile',
						pubkey: data.pubkey,
						profile,
						profileEvent
					};
				} else if (decoded.type === 'naddr') {
					// Fetch addressable event with relay hints
					const data = decoded.data as {
						kind: number;
						pubkey: string;
						identifier: string;
						relays?: string[];
					};

					const events = await loadWithRouter({
						filters: [
							{
								kinds: [data.kind],
								authors: [data.pubkey],
								'#d': [data.identifier]
							}
						],
						relayHints: data.relays
					});

					return {
						type: 'note',
						event: events[0] || null
					};
				}

				return null;
			},
			enabled: !!decoded
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
			{#if isInvalidIdentifier}
				<!-- Invalid NIP-19 identifier -->
				<div class="p-6">
					<Card.Root class="border-destructive">
						<Card.Content class="pt-6">
							<p class="text-destructive">Invalid Nostr identifier</p>
							<p class="text-sm text-muted-foreground mt-2">
								The identifier "{$page.params.nip19}" is not a valid NIP-19 identifier.
							</p>
							<Button class="mt-4" onclick={() => goto('/')}>
								Go Home
							</Button>
						</Card.Content>
					</Card.Root>
				</div>
			{:else if contentQuery.isLoading}
				<!-- Loading state -->
				<div class="p-6 space-y-4">
					<Skeleton class="h-32 w-full" />
					<Skeleton class="h-48 w-full" />
				</div>
			{:else if contentQuery.error}
				<!-- Error state -->
				<div class="p-6">
					<Card.Root class="border-destructive">
						<Card.Content class="pt-6">
							<p class="text-destructive">Failed to load content: {contentQuery.error.message}</p>
							<Button class="mt-4" onclick={() => goto('/')}>
								Go Home
							</Button>
						</Card.Content>
					</Card.Root>
				</div>
			{:else if contentQuery.data}
				{@const data = contentQuery.data as NIP19Data}

				{#if data && data.type === 'profile'}
					{@const profileData = data}
					{@const { followingCount } = useFollowing(profileData.pubkey)}
					{@const profileStatsQuery = useProfileStats(profileData.pubkey)}
					{@const profileStats = profileStatsQuery.data}

					{@const postsQuery = createInfiniteQuery(() => ({
						queryKey: ['profile-posts', profileData.pubkey, profileFilter],
						queryFn: async ({ pageParam, signal }) => {
							let filters: any[] = [];

							if (profileFilter === 'posts') {
								// Top-level kind 1 posts (no #e tags = not a reply)
								const events = await loadWithRouter({
									filters: [{
										kinds: [1],
										authors: [profileData.pubkey],
										limit: 20,
										...(pageParam ? { until: pageParam } : {})
									}],
									signal,
								});
								// Filter out replies (events with #e tags)
								return events
									.filter(e => !e.tags.some(tag => tag[0] === 'e'))
									.sort((a, b) => b.created_at - a.created_at);
							} else if (profileFilter === 'replies') {
								// Kind 1 posts with #e tags (replies)
								const events = await loadWithRouter({
									filters: [{
										kinds: [1],
										authors: [profileData.pubkey],
										limit: 20,
										...(pageParam ? { until: pageParam } : {})
									}],
									signal,
								});
								// Filter for replies (events with #e tags)
								return events
									.filter(e => e.tags.some(tag => tag[0] === 'e'))
									.sort((a, b) => b.created_at - a.created_at);
							} else if (profileFilter === 'articles') {
								// Long-form articles (kind 30023)
								filters = [{
									kinds: [30023],
									authors: [profileData.pubkey],
									limit: 20,
									...(pageParam ? { until: pageParam } : {})
								}];
							} else if (profileFilter === 'media') {
								// Media events (kind 21, 22)
								filters = [{
									kinds: [21, 22],
									authors: [profileData.pubkey],
									limit: 20,
									...(pageParam ? { until: pageParam } : {})
								}];
							} else if (profileFilter === 'likes') {
								// Reactions/likes by this user (kind 7)
								filters = [{
									kinds: [7],
									authors: [profileData.pubkey],
									limit: 20,
									...(pageParam ? { until: pageParam } : {})
								}];
							}

							if (filters.length > 0) {
								const events = await loadWithRouter({
									filters,
									signal,
								});
								return events.sort((a, b) => b.created_at - a.created_at);
							}

							return [];
						},
						initialPageParam: null as number | null,
						getNextPageParam: (lastPage) => {
							if (lastPage.length === 0) return null;
							return lastPage[lastPage.length - 1].created_at;
						}
					}))}

					{@const allPosts = postsQuery.data?.pages.flatMap((page) => page) || []}
					{@const displayName = profileData.profile.display_name || profileData.profile.name || genUserName(profileData.pubkey)}
					{@const username = profileData.profile.name || `@${$page.params.nip19?.slice(0, 12)}...`}
					{@const bio = profileData.profile.about}
					{@const avatarUrl = profileData.profile.picture}
					{@const bannerUrl = profileData.profile.banner}
					{@const website = profileData.profile.website}
					{@const joinedDate = profileData.profileEvent ? new Date(profileData.profileEvent.created_at * 1000).toLocaleDateString('en-US', { month: 'long', year: 'numeric' }) : null}
					{@const isOwnProfile = $currentUser?.pubkey === profileData.pubkey}

					<!-- Profile view -->
					<div class="min-h-screen">
						<!-- Header -->
						<div class="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
							<div class="flex items-center gap-4 p-4">
								<a href="/">
									<Button variant="ghost" size="icon" class="rounded-full">
										<ArrowLeft class="h-5 w-5" />
									</Button>
								</a>
								<div>
									<h1 class="text-xl font-bold">{displayName}</h1>
									<p class="text-sm text-muted-foreground">{allPosts.length} posts</p>
								</div>
							</div>
						</div>

						<!-- Banner -->
						<div class="relative">
							{#if bannerUrl}
								<img
									src={bannerUrl}
									alt="Banner"
									class="w-full h-48 object-cover bg-muted"
								/>
							{:else}
								<div class="w-full h-48 bg-gradient-to-br from-blue-400 to-blue-600"></div>
							{/if}
						</div>

						<!-- Profile Info -->
						<div class="px-4 pb-4">
							<!-- Avatar & Actions -->
							<div class="flex items-start justify-between mb-4">
								<Avatar class="w-32 h-32 -mt-16 border-4 border-background">
									<AvatarImage src={avatarUrl} alt={displayName} />
									<AvatarFallback class="text-4xl">{displayName[0]?.toUpperCase() || 'A'}</AvatarFallback>
								</Avatar>

								<div class="mt-3 flex gap-2">
									{#if isOwnProfile}
										<a href="/settings">
											<Button variant="outline" class="rounded-full px-6">
												<Settings2 class="h-4 w-4 mr-2" />
												Edit Profile
											</Button>
										</a>
									{:else}
										<FollowButton pubkey={profileData.pubkey} />
									{/if}
								</div>
							</div>

							<!-- Name & Username -->
							<div class="mb-3">
								<h2 class="text-2xl font-bold">{displayName}</h2>
								<p class="text-muted-foreground">{username}</p>
							</div>

							<!-- Bio -->
							{#if bio}
								<p class="text-base mb-3 whitespace-pre-wrap">{bio}</p>
							{/if}

							<!-- Metadata -->
							<div class="flex flex-wrap gap-4 text-sm text-muted-foreground mb-3">
								{#if website}
									<a
										href={website}
										target="_blank"
										rel="noopener noreferrer"
										class="hover:underline text-blue-500"
									>
										{website.replace(/^https?:\/\//, '')}
									</a>
								{/if}
								{#if joinedDate}
									<span class="flex items-center gap-1">
										<Calendar class="h-4 w-4" />
										Joined {joinedDate}
									</span>
								{/if}
							</div>

							<!-- Following / Followers -->
							<div class="flex gap-4 text-sm">
								<span>
									<strong class="font-bold text-foreground">{followingCount}</strong>{' '}
									<span class="text-muted-foreground">Following</span>
								</span>
								<span>
									<strong class="font-bold text-foreground">
										{profileStats?.followers_pubkey_count?.toLocaleString() || '0'}
									</strong>{' '}
									<span class="text-muted-foreground">Followers</span>
								</span>
							</div>
						</div>

						<!-- Content Tabs -->
						<div class="border-t border-border">
							<div class="flex items-center gap-1 overflow-x-auto">
								<button
									onclick={() => (profileFilter = 'posts')}
									class={cn(
										'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
										profileFilter === 'posts' ? 'text-foreground' : 'text-muted-foreground'
									)}
								>
									Posts
									{#if profileFilter === 'posts'}
										<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
									{/if}
								</button>
								<button
									onclick={() => (profileFilter = 'replies')}
									class={cn(
										'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
										profileFilter === 'replies' ? 'text-foreground' : 'text-muted-foreground'
									)}
								>
									Replies
									{#if profileFilter === 'replies'}
										<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
									{/if}
								</button>
								<button
									onclick={() => (profileFilter = 'articles')}
									class={cn(
										'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
										profileFilter === 'articles' ? 'text-foreground' : 'text-muted-foreground'
									)}
								>
									Articles
									{#if profileFilter === 'articles'}
										<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
									{/if}
								</button>
								<button
									onclick={() => (profileFilter = 'media')}
									class={cn(
										'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
										profileFilter === 'media' ? 'text-foreground' : 'text-muted-foreground'
									)}
								>
									Media
									{#if profileFilter === 'media'}
										<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
									{/if}
								</button>
								<button
									onclick={() => (profileFilter = 'likes')}
									class={cn(
										'px-4 py-3 font-semibold text-center hover:bg-accent/50 transition-colors relative whitespace-nowrap text-sm',
										profileFilter === 'likes' ? 'text-foreground' : 'text-muted-foreground'
									)}
								>
									Likes
									{#if profileFilter === 'likes'}
										<div class="absolute bottom-0 left-0 right-0 h-1 bg-blue-500 rounded-full"></div>
									{/if}
								</button>
							</div>
						</div>

						<!-- Posts Content -->
						<div class="border-t border-border">
							{#if postsQuery.isLoading}
								<div class="flex items-center justify-center py-20">
									<Loader2 class="h-8 w-8 animate-spin text-blue-500" />
								</div>
							{:else if allPosts.length === 0}
								<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
									<p class="text-xl font-bold mb-2">
										{#if profileFilter === 'posts'}
											No posts yet
										{:else if profileFilter === 'replies'}
											No replies yet
										{:else if profileFilter === 'articles'}
											No articles yet
										{:else if profileFilter === 'media'}
											No media yet
										{:else if profileFilter === 'likes'}
											No likes yet
										{/if}
									</p>
									<p class="text-muted-foreground">
										{#if isOwnProfile}
											{#if profileFilter === 'posts'}
												You haven't posted anything yet.
											{:else if profileFilter === 'replies'}
												You haven't replied to any posts yet.
											{:else if profileFilter === 'articles'}
												You haven't published any articles yet.
											{:else if profileFilter === 'media'}
												You haven't shared any media yet.
											{:else if profileFilter === 'likes'}
												You haven't liked any posts yet.
											{/if}
										{:else}
											{#if profileFilter === 'posts'}
												This user hasn't posted anything yet.
											{:else if profileFilter === 'replies'}
												This user hasn't replied to any posts yet.
											{:else if profileFilter === 'articles'}
												This user hasn't published any articles yet.
											{:else if profileFilter === 'media'}
												This user hasn't shared any media yet.
											{:else if profileFilter === 'likes'}
												This user hasn't liked any posts yet.
											{/if}
										{/if}
									</p>
								</div>
							{:else}
								{#each allPosts as event (event.id)}
									<Note {event} />
								{/each}

								<!-- Infinite scroll trigger -->
								<div class="py-8 flex justify-center">
									{#if postsQuery.isFetchingNextPage}
										<Loader2 class="h-6 w-6 animate-spin text-blue-500" />
									{:else if postsQuery.hasNextPage}
										<Button
											variant="ghost"
											onclick={() => postsQuery.fetchNextPage()}
											disabled={postsQuery.isFetchingNextPage}
										>
											Load more
										</Button>
									{:else if allPosts.length > 0}
										<p class="text-muted-foreground text-sm">No more posts</p>
									{/if}
								</div>
							{/if}
						</div>
					</div>
				{:else if data && data.type === 'note'}
					{@const noteData = data}
					<!-- Thread view for note -->
					{#if noteData.event}
						{@const mainEvent = noteData.event}
						{@const parentEventIds = mainEvent.tags
							.filter(tag => tag[0] === 'e')
							.map(tag => tag[1])}

						<!-- Fetch parent events for thread context -->
						{@const parentQuery = createQuery(() => ({
							queryKey: ['thread-parents', parentEventIds],
							queryFn: async ({ signal }) => {
								if (parentEventIds.length === 0) return [];

								const events = await loadWithRouter({
									filters: [{ ids: parentEventIds, kinds: [1] }],
									signal,
								});

								// Sort by created_at ascending (oldest first) to show conversation order
								return events.sort((a, b) => a.created_at - b.created_at);
							},
							enabled: parentEventIds.length > 0
						}))}

						<!-- Fetch replies to main event -->
						{@const repliesQuery = createQuery(() => ({
							queryKey: ['replies', mainEvent.id],
							queryFn: async ({ signal }) => {
								const replyEvents = await loadWithRouter({
									filters: [{ kinds: [1], '#e': [mainEvent.id], limit: 100 }],
									signal,
								});

								// Sort by created_at ascending (oldest first)
								return replyEvents.sort((a, b) => a.created_at - b.created_at);
							}
						}))}

						<div class="min-h-screen">
							<!-- Header -->
							<div class="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
								<div class="flex items-center gap-4 p-4">
									<a href="/">
										<Button variant="ghost" size="icon" class="rounded-full">
											<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m15 18-6-6 6-6"/></svg>
										</Button>
									</a>
									<h1 class="text-xl font-bold">Post</h1>
								</div>
							</div>

							<!-- Parent posts (thread context) -->
							{#if parentQuery.data && parentQuery.data.length > 0}
								<div class="border-b-2 border-blue-500/20">
									{#each parentQuery.data as parentEvent (parentEvent.id)}
										<div class="relative">
											<Note event={parentEvent} showThread={false} />
											<!-- Thread line indicator -->
											<div class="absolute left-[40px] top-[60px] bottom-0 w-0.5 bg-border"></div>
										</div>
									{/each}
								</div>
							{/if}

							<!-- Main post being viewed -->
							<Note event={mainEvent} showThread={false} />

							<div class="border-b border-border"></div>

							<!-- Reply Composer -->
							<div class="border-b border-border p-4">
								<ReplyComposer replyToEventId={mainEvent.id} replyToAuthor={mainEvent.pubkey} />
							</div>

							<!-- Replies -->
							{#if repliesQuery.isLoading}
								<div class="flex items-center justify-center py-10">
									<svg class="h-6 w-6 animate-spin text-blue-500" xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24">
										<circle class="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" stroke-width="4"></circle>
										<path class="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z"></path>
									</svg>
								</div>
							{:else if repliesQuery.data && repliesQuery.data.length > 0}
								<div>
									{#each repliesQuery.data as reply (reply.id)}
										<Note event={reply} />
									{/each}
								</div>
							{:else}
								<div class="flex flex-col items-center justify-center py-10 px-4 text-center text-muted-foreground">
									<p>No replies yet</p>
									<p class="text-sm">Be the first to reply!</p>
								</div>
							{/if}
						</div>
					{:else}
						<!-- Note not found -->
						<div class="p-6">
							<Card.Root class="border-dashed">
								<Card.Content class="py-12 text-center">
									<p class="text-muted-foreground">Post not found</p>
								</Card.Content>
							</Card.Root>
						</div>
					{/if}
				{/if}
			{/if}
		</div>
	{/snippet}
</MainLayout>
