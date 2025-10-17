<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { load } from '@welshman/net';
	import Note from '$lib/components/Note.svelte';
	import NoteComposer from '$lib/components/NoteComposer.svelte';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import * as Card from '$lib/components/ui/card';
	import { LoginArea } from '$lib/components/auth/LoginArea.svelte';
	import { currentUser } from '$lib/stores/auth';

	// Composer dialog state
	let composerOpen = $state(false);
	let replyToEventId = $state<string | undefined>(undefined);

	// Handle reply
	function handleReply(eventId: string) {
		replyToEventId = eventId;
		composerOpen = true;
	}

	// Handle composer close
	function handleComposerClose() {
		composerOpen = false;
		replyToEventId = undefined;
	}

	// Query for recent notes (kind 1)
	const feedQuery = createQuery(() => ({
		queryKey: ['feed', 'global'],
		queryFn: async ({ signal }) => {
			const events = await load({
				relays: [],
				filters: [
					{
						kinds: [1], // Text notes
						limit: 50
					}
				],
				signal
			});

			// Sort by created_at descending (newest first)
			return events.sort((a, b) => b.created_at - a.created_at);
		},
		staleTime: 30000, // 30 seconds
		refetchInterval: 60000 // Refetch every minute
	}));
</script>

<div class="min-h-screen bg-background">
	<!-- Header -->
	<header class="sticky top-0 z-50 w-full border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
		<div class="container flex h-14 items-center justify-between max-w-4xl mx-auto px-4">
			<div class="flex items-center gap-2">
				<h1 class="text-xl font-bold">PUStack</h1>
			</div>
			<LoginArea class="max-w-xs" />
		</div>
	</header>

	<!-- Main Content -->
	<main class="container max-w-4xl mx-auto px-4 py-6">
		<!-- Note Composer (for logged-in users) -->
		{#if $currentUser}
			<Card.Root class="mb-6">
				<Card.Content class="pt-6">
					<button
						class="w-full text-left px-4 py-3 border rounded-lg hover:bg-accent transition-colors"
						onclick={() => (composerOpen = true)}
					>
						<span class="text-muted-foreground">What's on your mind?</span>
					</button>
				</Card.Content>
			</Card.Root>
		{/if}

		<!-- Note Composer Dialog -->
		<NoteComposer bind:isOpen={composerOpen} replyTo={replyToEventId} onClose={handleComposerClose} />

		<!-- Feed -->
		<div class="space-y-4">
			{#if $feedQuery.isLoading}
				<!-- Loading skeletons -->
				{#each Array(5) as _, i}
					<Card.Root>
						<Card.Header class="flex flex-row items-start gap-3 space-y-0">
							<Skeleton class="h-10 w-10 rounded-full" />
							<div class="flex-1 space-y-2">
								<Skeleton class="h-4 w-32" />
								<Skeleton class="h-3 w-24" />
							</div>
						</Card.Header>
						<Card.Content>
							<div class="space-y-2">
								<Skeleton class="h-4 w-full" />
								<Skeleton class="h-4 w-5/6" />
								<Skeleton class="h-4 w-4/6" />
							</div>
						</Card.Content>
					</Card.Root>
				{/each}
			{:else if $feedQuery.error}
				<!-- Error state -->
				<Card.Root class="border-destructive">
					<Card.Content class="pt-6">
						<p class="text-destructive">
							Failed to load feed: {$feedQuery.error.message}
						</p>
						<button
							class="mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90"
							onclick={() => $feedQuery.refetch()}
						>
							Retry
						</button>
					</Card.Content>
				</Card.Root>
			{:else if $feedQuery.data && $feedQuery.data.length === 0}
				<!-- Empty state -->
				<Card.Root class="border-dashed">
					<Card.Content class="py-12 text-center">
						<p class="text-muted-foreground">
							No notes found. Try checking your relay connections.
						</p>
					</Card.Content>
				</Card.Root>
			{:else if $feedQuery.data}
				<!-- Notes list -->
				{#each $feedQuery.data as event (event.id)}
					<Note {event} showReplyButton={true} onReply={handleReply} />
				{/each}
			{/if}
		</div>
	</main>
</div>
