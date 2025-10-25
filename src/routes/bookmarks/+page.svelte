<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import BookmarkItem from '$lib/components/BookmarkItem.svelte';
	import { useUserBookmarks } from '$lib/hooks/useUserBookmarks.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { Bookmark, Loader2 } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';

	const { bookmarks, isLoading, isError, error, removeBookmark } = useUserBookmarks();

	function handleRemoveBookmark(type: string, value: string) {
		removeBookmark.mutate({ type, value });
	}
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
					<h1 class="text-xl font-bold flex items-center gap-2">
						<Bookmark class="w-6 h-6" />
						Bookmarks
					</h1>
					{#if $currentUser && bookmarks.length > 0}
						<p class="text-sm text-muted-foreground mt-1">
							{bookmarks.length} {bookmarks.length === 1 ? 'bookmark' : 'bookmarks'}
						</p>
					{/if}
				</div>
			</div>

			<!-- Content -->
			{#if !$currentUser}
				<!-- Not logged in -->
				<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
					<Bookmark class="w-16 h-16 mb-4 text-muted-foreground" />
					<h2 class="text-2xl font-bold mb-2">Sign in to see your bookmarks</h2>
					<p class="text-muted-foreground max-w-sm">
						Connect your Nostr account to save and view bookmarks of posts, articles, hashtags, and
						more.
					</p>
				</div>
			{:else if isLoading}
				<!-- Loading -->
				<div class="flex items-center justify-center py-20">
					<Loader2 class="w-8 h-8 animate-spin text-blue-500" />
				</div>
			{:else if isError}
				<!-- Error -->
				<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
					<div class="text-6xl mb-4">⚠️</div>
					<h2 class="text-2xl font-bold mb-2">Error loading bookmarks</h2>
					<p class="text-muted-foreground max-w-sm mb-6">
						{error?.message || 'Failed to load bookmarks. Please try again.'}
					</p>
				</div>
			{:else if bookmarks.length === 0}
				<!-- Empty state -->
				<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
					<Bookmark class="w-16 h-16 mb-4 text-muted-foreground" />
					<h2 class="text-2xl font-bold mb-2">No bookmarks yet</h2>
					<p class="text-muted-foreground max-w-sm">
						Save posts, articles, hashtags, and links you want to revisit later. Click the
						bookmark icon on any post to add it here.
					</p>
				</div>
			{:else}
				<!-- Bookmarks list -->
				<div>
					{#each bookmarks as bookmark, index (`${bookmark.type}-${bookmark.value}-${index}`)}
						<BookmarkItem {bookmark} onRemove={() => handleRemoveBookmark(bookmark.type, bookmark.value)} />
					{/each}
				</div>
			{/if}
		</div>
	{/snippet}
</MainLayout>
