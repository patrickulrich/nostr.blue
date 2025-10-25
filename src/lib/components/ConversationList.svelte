<script lang="ts">
	import { MessageCircle } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as ScrollArea from '$lib/components/ui/scroll-area';
	import { createQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import type { Conversation } from '$lib/stores/messages.svelte';
	import { nip19 } from 'nostr-tools';
	import { cn } from '$lib/utils';

	let {
		conversations,
		selectedPubkey,
		onSelectConversation
	}: {
		conversations: Conversation[];
		selectedPubkey?: string;
		onSelectConversation: (pubkey: string) => void;
	} = $props();

	function formatTimestamp(timestamp: number): string {
		const now = Date.now();
		const diffMs = now - timestamp * 1000;
		const diffMins = Math.floor(diffMs / 60000);
		const diffHours = Math.floor(diffMs / 3600000);
		const diffDays = Math.floor(diffMs / 86400000);

		if (diffMins < 1) return 'just now';
		if (diffMins < 60) return `${diffMins}m`;
		if (diffHours < 24) return `${diffHours}h`;
		if (diffDays < 7) return `${diffDays}d`;
		return new Date(timestamp * 1000).toLocaleDateString();
	}

	function truncateMessage(content: string | undefined, maxLength: number = 60): string {
		if (!content) return '';
		if (content.length <= maxLength) return content;
		return content.slice(0, maxLength) + '...';
	}
</script>

{#if conversations.length === 0}
	<div class="flex flex-col items-center justify-center h-full p-8 text-center">
		<MessageCircle class="h-12 w-12 text-muted-foreground mb-4" />
		<h3 class="font-semibold text-lg mb-2">No conversations yet</h3>
		<p class="text-sm text-muted-foreground max-w-sm">
			Start a conversation by visiting someone's profile and sending them a message.
		</p>
	</div>
{:else}
	<ScrollArea.Root class="h-full">
		<div class="h-full">
			<div class="space-y-1 p-2">
				{#each conversations as conversation (conversation.pubkey)}
					{@const npub = nip19.npubEncode(conversation.pubkey)}
					{@const authorQuery = createQuery(() => ({
						queryKey: ['profile', conversation.pubkey],
						queryFn: async ({ signal }) => {
							const events = await loadWithRouter({
								filters: [{ kinds: [0], authors: [conversation.pubkey], limit: 1 }],
								signal
							});
							if (events.length === 0) return null;
							try {
								return JSON.parse(events[0].content);
							} catch {
								return null;
							}
						},
						staleTime: 5 * 60 * 1000
					}))}
					{@const displayName =
						authorQuery.data?.display_name || authorQuery.data?.name || npub.slice(0, 12)}
					{@const avatarUrl = authorQuery.data?.picture}
					{@const messagePreview = truncateMessage(conversation.lastMessage.content)}
					{@const timeAgo = formatTimestamp(conversation.lastMessage.created_at)}

					<Button
						variant="ghost"
						class={cn(
							'w-full justify-start h-auto py-3 px-4 hover:bg-muted/50',
							selectedPubkey === conversation.pubkey && 'bg-muted'
						)}
						onclick={() => onSelectConversation(conversation.pubkey)}
					>
						<div class="flex items-start gap-3 w-full">
							<img
								src={avatarUrl || `https://api.dicebear.com/7.x/identicon/svg?seed=${conversation.pubkey}`}
								alt={displayName}
								class="h-12 w-12 rounded-full flex-shrink-0"
							/>

							<div class="flex-1 min-w-0 text-left">
								<div class="flex items-center justify-between gap-2">
									<span class="font-semibold text-sm truncate">
										{displayName}
									</span>
									<span class="text-xs text-muted-foreground flex-shrink-0">
										{timeAgo}
									</span>
								</div>

								<p class="text-sm text-muted-foreground truncate mt-1">
									{#if conversation.lastMessage.isSent}
										<span class="text-foreground/70">You: </span>
									{/if}
									{messagePreview}
								</p>
							</div>
						</div>
					</Button>
				{/each}
			</div>
		</div>
		<ScrollArea.Scrollbar orientation="vertical" />
	</ScrollArea.Root>
{/if}
