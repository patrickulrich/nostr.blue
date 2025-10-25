<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { useUserLists, getListTypeName, getListIcon, useDeleteList } from '$lib/hooks/useLists.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { List, Plus, Loader2, Trash2, Users, Link, Bookmark, BookMarked } from 'lucide-svelte';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { goto } from '$app/navigation';

	const listsQuery = useUserLists();
	const deleteList = useDeleteList();

	let deleteConfirmOpen = $state(false);
	let listToDelete = $state<any>(null);

	function handleDelete() {
		if (listToDelete) {
			deleteList.mutate({ event: listToDelete.event });
			deleteConfirmOpen = false;
			listToDelete = null;
		}
	}

	function confirmDelete(list: any) {
		listToDelete = list;
		deleteConfirmOpen = true;
	}

	function getItemCount(list: any): number {
		return list.tags.filter((tag: string[]) => ['p', 'e', 'r', 't', 'a'].includes(tag[0])).length;
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
				<div class="px-4 pt-3 flex items-center justify-between">
					<h1 class="text-xl font-bold px-4 py-3 flex items-center gap-2">
						<List class="w-6 h-6" />
						Lists
					</h1>
				</div>
			</div>

			<!-- Content -->
			<div class="p-4">
				{#if !$currentUser}
					<!-- Not logged in -->
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<List class="w-16 h-16 mb-4 text-muted-foreground" />
						<h2 class="text-2xl font-bold mb-2">Organize your Nostr</h2>
						<p class="text-muted-foreground max-w-sm mb-6">
							Log in to create and manage custom lists of people, relays, and content.
						</p>
					</div>
				{:else if listsQuery.isLoading}
					<!-- Loading -->
					<div class="flex flex-col items-center justify-center py-20 px-4">
						<Loader2 class="w-8 h-8 animate-spin text-muted-foreground mb-4" />
						<p class="text-muted-foreground">Loading your lists...</p>
					</div>
				{:else if listsQuery.isError}
					<!-- Error -->
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<div class="text-6xl mb-4">⚠️</div>
						<h2 class="text-2xl font-bold mb-2">Error loading lists</h2>
						<p class="text-muted-foreground max-w-sm mb-6">
							{listsQuery.error?.message || 'Failed to load lists. Please try again.'}
						</p>
						<Button onclick={() => listsQuery.refetch()}>Try Again</Button>
					</div>
				{:else if listsQuery.data && listsQuery.data.length === 0}
					<!-- Empty state -->
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<List class="w-16 h-16 mb-4 text-muted-foreground" />
						<h2 class="text-2xl font-bold mb-2">No lists yet</h2>
						<p class="text-muted-foreground max-w-sm mb-6">
							Create custom lists to organize people, relays, bookmarks, or curated content.
						</p>
						<div class="text-left space-y-4 max-w-md">
							<div class="flex items-start gap-3 p-3 bg-muted/50 rounded-lg">
								<Users class="w-5 h-5 mt-1" />
								<div>
									<h3 class="font-semibold">People Lists</h3>
									<p class="text-sm text-muted-foreground">
										Organize contacts into groups (friends, work, etc.)
									</p>
								</div>
							</div>
							<div class="flex items-start gap-3 p-3 bg-muted/50 rounded-lg">
								<Link class="w-5 h-5 mt-1" />
								<div>
									<h3 class="font-semibold">Relay Lists</h3>
									<p class="text-muted-foreground text-sm text-muted-foreground">
										Save and share your favorite relay configurations
									</p>
								</div>
							</div>
							<div class="flex items-start gap-3 p-3 bg-muted/50 rounded-lg">
								<Bookmark class="w-5 h-5 mt-1" />
								<div>
									<h3 class="font-semibold">Bookmark Collections</h3>
									<p class="text-sm text-muted-foreground">
										Organize your saved posts into categories
									</p>
								</div>
							</div>
							<div class="flex items-start gap-3 p-3 bg-muted/50 rounded-lg">
								<BookMarked class="w-5 h-5 mt-1" />
								<div>
									<h3 class="font-semibold">Content Curations</h3>
									<p class="text-sm text-muted-foreground">
										Curate and share collections of notes
									</p>
								</div>
							</div>
						</div>
					</div>
				{:else if listsQuery.data}
					<!-- Lists grid -->
					<div class="grid gap-4 grid-cols-1 md:grid-cols-2 lg:grid-cols-3">
						{#each listsQuery.data as list (list.id)}
							<Card.Root class="hover:shadow-md transition-shadow">
								<Card.Header>
									<Card.Title class="flex items-center gap-2 text-lg">
										<span class="text-2xl">{getListIcon(list.kind)}</span>
										{list.name}
									</Card.Title>
									<Card.Description class="text-xs text-muted-foreground">
										{getListTypeName(list.kind)} • {getItemCount(list)} items
									</Card.Description>
								</Card.Header>
								{#if list.description}
									<Card.Content>
										<p class="text-sm text-muted-foreground line-clamp-2">
											{list.description}
										</p>
									</Card.Content>
								{/if}
								<Card.Footer class="flex gap-2">
									<Button
										variant="outline"
										size="sm"
										class="flex-1"
										onclick={() => goto(`/lists/${list.identifier}`)}
									>
										View
									</Button>
									<Button
										variant="ghost"
										size="sm"
										onclick={() => confirmDelete(list)}
									>
										<Trash2 class="w-4 h-4 text-destructive" />
									</Button>
								</Card.Footer>
							</Card.Root>
						{/each}
					</div>
				{/if}
			</div>
		</div>

		<!-- Delete Confirmation Dialog -->
		<Dialog.Root bind:open={deleteConfirmOpen}>
			<Dialog.Content>
				<Dialog.Header>
					<Dialog.Title>Delete List?</Dialog.Title>
					<Dialog.Description>
						Are you sure you want to delete "{listToDelete?.name}"? This action cannot be undone.
					</Dialog.Description>
				</Dialog.Header>
				<Dialog.Footer class="flex gap-2">
					<Button variant="outline" onclick={() => (deleteConfirmOpen = false)}>
						Cancel
					</Button>
					<Button
						variant="destructive"
						onclick={handleDelete}
						disabled={deleteList.isPending}
					>
						{#if deleteList.isPending}
							<Loader2 class="w-4 h-4 animate-spin mr-2" />
						{/if}
						Delete
					</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/snippet}
</MainLayout>
