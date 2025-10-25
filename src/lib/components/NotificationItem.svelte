<script lang="ts">
	import { Heart, Repeat2, MessageCircle, Zap, AtSign } from 'lucide-svelte';
	import { nip19 } from 'nostr-tools';
	import type { NotificationEvent } from '$lib/stores/notifications.svelte';
	import { createQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import { cn } from '$lib/utils';

	let { notification, class: className }: { notification: NotificationEvent; class?: string } =
		$props();

	const { event, type, targetEventId } = notification;

	// Fetch author profile
	const authorQuery = createQuery(() => ({
		queryKey: ['profile', event.pubkey],
		queryFn: async ({ signal }) => {
			const events = await loadWithRouter({
				filters: [{ kinds: [0], authors: [event.pubkey], limit: 1 }],
				signal
			});
			if (events.length === 0) return null;
			try {
				return JSON.parse(events[0].content);
			} catch {
				return null;
			}
		},
		staleTime: 5 * 60 * 1000 // 5 minutes
	}));

	const npub = nip19.npubEncode(event.pubkey);
	let displayName = $derived(authorQuery.data?.display_name || authorQuery.data?.name || npub.slice(0, 12));
	let avatarUrl = $derived(authorQuery.data?.picture);

	function formatTimestamp(timestamp: number): string {
		const now = Date.now();
		const diffMs = now - timestamp * 1000;
		const diffMins = Math.floor(diffMs / 60000);
		const diffHours = Math.floor(diffMs / 3600000);
		const diffDays = Math.floor(diffMs / 86400000);

		if (diffMins < 1) return 'just now';
		if (diffMins < 60) return `${diffMins}m ago`;
		if (diffHours < 24) return `${diffHours}h ago`;
		return `${diffDays}d ago`;
	}

	const timestamp = formatTimestamp(event.created_at);

	// Get the notification link
	let linkTo = $derived.by(() => {
		if (type === 'reply') {
			return `/${nip19.noteEncode(event.id)}`; // Link to the reply itself
		} else if (targetEventId) {
			return `/${nip19.noteEncode(targetEventId)}`; // Link to the target post
		} else {
			return `/${npub}`; // Fallback to profile
		}
	});

	function getNotificationText(): string {
		switch (type) {
			case 'reaction':
				return 'liked your post';
			case 'repost':
				return 'reposted your post';
			case 'reply':
				return 'replied to your post';
			case 'zap':
				return 'zapped your post';
			case 'mention':
				return 'mentioned you';
			default:
				return 'interacted with your post';
		}
	}
</script>

<a
	href={linkTo}
	class={cn(
		'flex gap-3 p-4 border-b border-border hover:bg-accent/50 transition-colors',
		className
	)}
>
	<!-- Icon -->
	<div class="flex-shrink-0 w-12 flex justify-center pt-1">
		{#if type === 'reaction'}
			<Heart class="h-8 w-8 fill-pink-500 text-pink-500" />
		{:else if type === 'repost'}
			<Repeat2 class="h-8 w-8 text-green-500" />
		{:else if type === 'reply'}
			<MessageCircle class="h-8 w-8 text-blue-500" />
		{:else if type === 'zap'}
			<Zap class="h-8 w-8 fill-amber-500 text-amber-500" />
		{:else if type === 'mention'}
			<AtSign class="h-8 w-8 text-blue-500" />
		{/if}
	</div>

	<!-- Content -->
	<div class="flex-1 min-w-0">
		<!-- Avatar and name -->
		<div class="flex items-start gap-2 mb-2">
			<img
				src={avatarUrl || `https://api.dicebear.com/7.x/identicon/svg?seed=${event.pubkey}`}
				alt={displayName}
				class="w-8 h-8 rounded-full flex-shrink-0"
			/>
			<div class="flex-1 min-w-0">
				<div class="flex items-center gap-2 flex-wrap">
					<span class="font-bold hover:underline">{displayName}</span>
					<span class="text-muted-foreground text-sm">{getNotificationText()}</span>
					<span class="text-muted-foreground text-sm">· {timestamp}</span>
				</div>
			</div>
		</div>

		<!-- Content preview (for replies and mentions) -->
		{#if (type === 'reply' || type === 'mention') && event.content}
			<div class="mt-2 text-sm text-muted-foreground line-clamp-3 pl-10">
				{event.content}
			</div>
		{/if}

		<!-- Reaction emoji -->
		{#if type === 'reaction'}
			<div class="mt-1 text-2xl pl-10">
				{event.content || '❤️'}
			</div>
		{/if}
	</div>
</a>
