<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { nip19 } from 'nostr-tools';
	import { createQuery } from '@tanstack/svelte-query';
	import { load } from '@welshman/net';
	import type { TrustedEvent } from '@welshman/util';
	import Note from '$lib/components/Note.svelte';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import * as Card from '$lib/components/ui/card';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import { genUserName } from '$lib/genUserName';

	// Define discriminated union types for the query data
	type ProfileData = {
		type: 'profile';
		pubkey: string;
		profile: Record<string, any>;
		notes: TrustedEvent[];
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
			goto('/404');
			return null;
		}
	});

	// Query based on decoded type
	// @ts-expect-error - TanStack Query in Svelte requires createQuery to be called within component context.
	const contentQuery = createQuery(() => ({
		queryKey: ['nip19', $page.params.nip19] as const,
		queryFn: async () => {
			if (!decoded) return null;

			if (decoded.type === 'npub') {
				// Fetch profile for npub
				const pubkey = decoded.data as string;
				const profiles = await load({
					relays: [],
					filters: [
						{
							kinds: [0],
							authors: [pubkey],
							limit: 1
						}
					]
				});

				const profile = profiles.length > 0 ? JSON.parse(profiles[0].content) : {};

				// Also fetch user's notes
				const notes = await load({
					relays: [],
					filters: [
						{
							kinds: [1],
							authors: [pubkey],
							limit: 20
						}
					]
				});

				return {
					type: 'profile',
					pubkey,
					profile,
					notes: notes.sort((a, b) => b.created_at - a.created_at)
				};
			} else if (decoded.type === 'note') {
				// Fetch single note
				const noteId = decoded.data as string;
				const events = await load({
					relays: [],
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
				// Fetch event with hints
				const data = decoded.data as { id: string; relays?: string[]; author?: string };
				const events = await load({
					relays: data.relays || [],
					filters: [
						{
							ids: [data.id]
						}
					]
				});

				return {
					type: 'note',
					event: events[0] || null
				};
			} else if (decoded.type === 'nprofile') {
				// Fetch profile with hints
				const data = decoded.data as { pubkey: string; relays?: string[] };
				const profiles = await load({
					relays: data.relays || [],
					filters: [
						{
							kinds: [0],
							authors: [data.pubkey],
							limit: 1
						}
					]
				});

				const profile = profiles.length > 0 ? JSON.parse(profiles[0].content) : {};

				// Also fetch user's notes
				const notes = await load({
					relays: data.relays || [],
					filters: [
						{
							kinds: [1],
							authors: [data.pubkey],
							limit: 20
						}
					]
				});

				return {
					type: 'profile',
					pubkey: data.pubkey,
					profile,
					notes: notes.sort((a, b) => b.created_at - a.created_at)
				};
			} else if (decoded.type === 'naddr') {
				// Fetch addressable event
				const data = decoded.data as {
					kind: number;
					pubkey: string;
					identifier: string;
					relays?: string[];
				};

				const events = await load({
					relays: data.relays || [],
					filters: [
						{
							kinds: [data.kind],
							authors: [data.pubkey],
							'#d': [data.identifier]
						}
					]
				});

				return {
					type: 'note',
					event: events[0] || null
				};
			}

			return null;
		},
		enabled: !!decoded
	}));
</script>

<div class="min-h-screen bg-background">
	<main class="container max-w-4xl mx-auto px-4 py-6">
		{#if $contentQuery.isLoading}
			<!-- Loading state -->
			<div class="space-y-4">
				<Skeleton class="h-32 w-full" />
				<Skeleton class="h-48 w-full" />
			</div>
		{:else if $contentQuery.error}
			<!-- Error state -->
			<Card.Root class="border-destructive">
				<Card.Content class="pt-6">
					<p class="text-destructive">Failed to load content: {$contentQuery.error.message}</p>
					<button
						class="mt-4 px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90"
						onclick={() => goto('/')}
					>
						Go Home
					</button>
				</Card.Content>
			</Card.Root>
		{:else if $contentQuery.data}
			{@const data = $contentQuery.data as NIP19Data}

			{#if data && data.type === 'profile'}
				{@const profileData = data}
				<!-- Profile view -->
				<div class="space-y-6">
					<!-- Profile header -->
					<Card.Root>
						<Card.Content class="pt-6">
							<div class="flex items-start gap-4">
								<Avatar class="h-20 w-20">
									<AvatarImage src={profileData.profile.picture} alt={profileData.profile.name} />
									<AvatarFallback>
										{(profileData.profile.display_name || profileData.profile.name || genUserName(profileData.pubkey))
											.slice(0, 2)
											.toUpperCase()}
									</AvatarFallback>
								</Avatar>

								<div class="flex-1">
									<h1 class="text-2xl font-bold">
										{profileData.profile.display_name || profileData.profile.name || genUserName(profileData.pubkey)}
									</h1>
									{#if profileData.profile.name && profileData.profile.display_name}
										<p class="text-muted-foreground">@{profileData.profile.name}</p>
									{/if}
									{#if profileData.profile.about}
										<p class="mt-2 text-sm">{profileData.profile.about}</p>
									{/if}
									{#if profileData.profile.nip05}
										<p class="mt-1 text-xs text-muted-foreground">
											✓ {profileData.profile.nip05}
										</p>
									{/if}
								</div>
							</div>
						</Card.Content>
					</Card.Root>

					<!-- User's notes -->
					<div>
						<h2 class="text-xl font-semibold mb-4">Notes</h2>
						<div class="space-y-4">
							{#if profileData.notes.length === 0}
								<Card.Root class="border-dashed">
									<Card.Content class="py-12 text-center">
										<p class="text-muted-foreground">No notes yet</p>
									</Card.Content>
								</Card.Root>
							{:else}
								{#each profileData.notes as event (event.id)}
									<Note {event} />
								{/each}
							{/if}
						</div>
					</div>
				</div>
			{:else if data && data.type === 'note'}
				{@const noteData = data}
				<!-- Single note view -->
				{#if noteData.event}
					<Note event={noteData.event} showReplyButton={true} />
				{:else}
					<Card.Root class="border-dashed">
						<Card.Content class="py-12 text-center">
							<p class="text-muted-foreground">Note not found</p>
						</Card.Content>
					</Card.Root>
				{/if}
			{/if}
		{/if}
	</main>
</div>
