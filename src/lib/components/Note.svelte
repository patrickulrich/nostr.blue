<script lang="ts">
	import type { TrustedEvent } from '@welshman/util';
	import { createQuery, createMutation } from '@tanstack/svelte-query';
	import { load } from '@welshman/net';
	import { nip19 } from 'nostr-tools';
	import * as Card from '$lib/components/ui/card';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import { Skeleton } from '$lib/components/ui/skeleton';
	import NoteContent from './NoteContent.svelte';
	import { genUserName } from '$lib/genUserName';
	import { useNostrPublish } from '$lib/stores/publish.svelte';
	import { useToast } from '$lib/stores/toast.svelte';
	import { currentUser } from '$lib/stores/auth';

	interface Props {
		event: TrustedEvent;
		showReplyButton?: boolean;
		onReply?: (eventId: string) => void;
		class?: string;
	}

	let { event, showReplyButton = false, onReply, class: className = '' }: Props = $props();

	const toast = useToast();
	const publish = useNostrPublish();

	// Fetch author profile
	// @ts-expect-error - TanStack Query in Svelte requires createQuery to be called within component context.
	const authorQuery = createQuery(() => ({
		queryKey: ['profile', event.pubkey],
		queryFn: async () => {
			const profiles = await load({
				relays: [],
				filters: [
					{
						kinds: [0],
						authors: [event.pubkey],
						limit: 1
					}
				]
			});

			if (profiles.length > 0) {
				try {
					return JSON.parse(profiles[0].content);
				} catch {
					return {};
				}
			}
			return {};
		}
	}));

	// Generate npub for profile link
	let npub = $derived(nip19.npubEncode(event.pubkey));

	// Get display name from metadata
	// @ts-expect-error - Query data types need refinement
	let displayName = $derived(
		$authorQuery.data?.display_name || $authorQuery.data?.name || genUserName(event.pubkey)
	);

	// Get profile picture
	// @ts-expect-error - Query data types need refinement
	let profilePicture = $derived($authorQuery.data?.picture);

	// Format timestamp
	let formattedTime = $derived(
		new Date(event.created_at * 1000).toLocaleDateString('en-US', {
			month: 'short',
			day: 'numeric',
			year: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		})
	);

	// Query reactions for this note
	// @ts-expect-error - TanStack Query in Svelte requires createQuery to be called within component context.
	const reactionsQuery = createQuery(() => ({
		queryKey: ['reactions', event.id],
		queryFn: async () => {
			const reactions = await load({
				relays: [],
				filters: [
					{
						kinds: [7], // Reactions (NIP-25)
						'#e': [event.id]
					}
				]
			});

			const likes = reactions.filter((r) => r.content === '+' || r.content === '❤️');
			return {
				likes: likes.length,
				hasLiked: likes.some((r) => r.pubkey === $currentUser?.pubkey)
			};
		}
	}));

	// Like mutation
	const likeMutation = createMutation({
		mutationFn: async () => {
			if (!$currentUser) {
				// @ts-expect-error - Toast API types need alignment
				toast.error('Please log in to like notes');
				throw new Error('Not logged in');
			}

			await $publish.mutateAsync({
				kind: 7, // Reaction
				content: '+',
				tags: [
					['e', event.id],
					['p', event.pubkey]
				]
			});
		},
		onSuccess: () => {
			// @ts-expect-error - Toast API types need alignment
			toast.success('Liked!');
			$reactionsQuery.refetch();
		},
		onError: () => {
			// @ts-expect-error - Toast API types need alignment
			toast.error('Failed to like note');
		}
	});

	// Repost mutation
	const repostMutation = createMutation({
		mutationFn: async () => {
			if (!$currentUser) {
				// @ts-expect-error - Toast API types need alignment
				toast.error('Please log in to repost notes');
				throw new Error('Not logged in');
			}

			await $publish.mutateAsync({
				kind: 6, // Repost (NIP-18)
				content: JSON.stringify(event),
				tags: [
					['e', event.id],
					['p', event.pubkey]
				]
			});
		},
		onSuccess: () => {
			// @ts-expect-error - Toast API types need alignment
			toast.success('Reposted!');
		},
		onError: () => {
			// @ts-expect-error - Toast API types need alignment
			toast.error('Failed to repost note');
		}
	});

	function handleLike() {
		if ($reactionsQuery.data?.hasLiked) {
			toast.info('You already liked this note');
			return;
		}
		$likeMutation.mutate();
	}

	function handleRepost() {
		$repostMutation.mutate();
	}

	function handleReply() {
		if (onReply) {
			onReply(event.id);
		}
	}
</script>

<Card.Root class={className}>
	<Card.Header class="flex flex-row items-start gap-3 space-y-0">
		<a href="/{npub}" class="shrink-0">
			{#if $authorQuery.isLoading}
				<Skeleton class="h-10 w-10 rounded-full" />
			{:else}
				<Avatar>
					<AvatarImage src={profilePicture} alt={displayName} />
					<AvatarFallback>{displayName.slice(0, 2).toUpperCase()}</AvatarFallback>
				</Avatar>
			{/if}
		</a>

		<div class="flex-1 min-w-0">
			<div class="flex items-center gap-2">
				<a href="/{npub}" class="font-semibold hover:underline truncate">
					{#if $authorQuery.isLoading}
						<Skeleton class="h-4 w-24" />
					{:else}
						{displayName}
					{/if}
				</a>
				<span class="text-sm text-muted-foreground">·</span>
				<time class="text-sm text-muted-foreground whitespace-nowrap" datetime={new Date(event.created_at * 1000).toISOString()}>
					{formattedTime}
				</time>
			</div>
		</div>
	</Card.Header>

	<Card.Content>
		<NoteContent {event} />

		<!-- Reaction buttons -->
		<div class="mt-4 flex items-center gap-6">
			<!-- Reply -->
			{#if showReplyButton}
				<button
					class="flex items-center gap-1.5 text-sm text-muted-foreground hover:text-primary transition-colors group"
					onclick={handleReply}
					disabled={!$currentUser}
				>
					<svg
						class="w-4 h-4 group-hover:scale-110 transition-transform"
						fill="none"
						stroke="currentColor"
						viewBox="0 0 24 24"
					>
						<path
							stroke-linecap="round"
							stroke-linejoin="round"
							stroke-width="2"
							d="M3 10h10a8 8 0 018 8v2M3 10l6 6m-6-6l6-6"
						/>
					</svg>
					<span>Reply</span>
				</button>
			{/if}

			<!-- Like -->
			<button
				class="flex items-center gap-1.5 text-sm transition-colors group"
				class:text-muted-foreground={!$reactionsQuery.data?.hasLiked}
				class:hover:text-red-500={!$reactionsQuery.data?.hasLiked}
				class:text-red-500={$reactionsQuery.data?.hasLiked}
				onclick={handleLike}
				disabled={$likeMutation.isPending || !$currentUser}
			>
				<svg
					class="w-4 h-4 group-hover:scale-110 transition-transform"
					fill={$reactionsQuery.data?.hasLiked ? 'currentColor' : 'none'}
					stroke="currentColor"
					viewBox="0 0 24 24"
				>
					<path
						stroke-linecap="round"
						stroke-linejoin="round"
						stroke-width="2"
						d="M4.318 6.318a4.5 4.5 0 000 6.364L12 20.364l7.682-7.682a4.5 4.5 0 00-6.364-6.364L12 7.636l-1.318-1.318a4.5 4.5 0 00-6.364 0z"
					/>
				</svg>
				<span>{$reactionsQuery.data?.likes ?? 0}</span>
			</button>

			<!-- Repost -->
			<button
				class="flex items-center gap-1.5 text-sm text-muted-foreground hover:text-green-500 transition-colors group"
				onclick={handleRepost}
				disabled={$repostMutation.isPending || !$currentUser}
			>
				<svg
					class="w-4 h-4 group-hover:scale-110 transition-transform"
					fill="none"
					stroke="currentColor"
					viewBox="0 0 24 24"
				>
					<path
						stroke-linecap="round"
						stroke-linejoin="round"
						stroke-width="2"
						d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
					/>
				</svg>
				<span>Repost</span>
			</button>
		</div>
	</Card.Content>
</Card.Root>
