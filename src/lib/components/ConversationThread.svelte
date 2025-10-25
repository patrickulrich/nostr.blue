<script lang="ts">
	import { Send, MessageCircle } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import * as ScrollArea from '$lib/components/ui/scroll-area';
	import { Textarea } from '$lib/components/ui/textarea';
	import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import { currentUser, signer } from '$lib/stores/auth';
	import { publishThunk } from '@welshman/app';
	import { makeEvent } from '@welshman/util';
	import { routerContext } from '@welshman/router';
	import { presetRelays } from '$lib/stores/appStore';
	import { encryptMessage } from '$lib/stores/messages.svelte';
	import type { Conversation } from '$lib/stores/messages.svelte';
	import { nip19 } from 'nostr-tools';
	import { cn } from '$lib/utils';
	import { onMount } from 'svelte';

	let {
		conversation
	}: {
		conversation?: Conversation;
	} = $props();

	let messageText = $state('');
	let textareaRef: any = $state(null);
	let scrollViewportRef: HTMLDivElement | null = $state(null);

	const queryClient = useQueryClient();

	// Send message mutation
	const sendMessageMutation = createMutation(() => ({
		mutationFn: async ({ recipientPubkey, content }: { recipientPubkey: string; content: string }) => {
			if (!$currentUser || !$signer) {
				throw new Error('Must be logged in to send messages');
			}

			// Encrypt the message
			const encryptedContent = await encryptMessage($signer, recipientPubkey, content);

			// Create the event
			const event = makeEvent(4, {
				content: encryptedContent,
				tags: [['p', recipientPubkey]]
			});

			// Determine relays to publish to
			const relays = routerContext.getDefaultRelays?.() || presetRelays.map(r => r.url);

			// Publish to relays
			publishThunk({ event, relays });

			return event;
		},
		onSuccess: () => {
			// Invalidate the direct messages query to refetch
			queryClient.invalidateQueries({ queryKey: ['direct-messages'] });
		}
	}));

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

	function handleSendMessage() {
		if (!messageText.trim() || !$currentUser || !conversation) return;

		sendMessageMutation.mutate(
			{
				recipientPubkey: conversation.pubkey,
				content: messageText.trim()
			},
			{
				onSuccess: () => {
					messageText = '';
					textareaRef?.focus();
				}
			}
		);
	}

	function handleKeyDown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			handleSendMessage();
		}
	}

	// Auto-scroll to bottom when messages change
	$effect(() => {
		if (conversation && scrollViewportRef) {
			scrollViewportRef.scrollTop = scrollViewportRef.scrollHeight;
		}
	});
</script>

{#if !conversation}
	<!-- Empty state -->
	<div class="flex flex-col items-center justify-center h-full p-8 text-center">
		<MessageCircle class="h-16 w-16 text-muted-foreground mb-4" />
		<h3 class="font-semibold text-xl mb-2">Select a conversation</h3>
		<p class="text-sm text-muted-foreground max-w-sm">
			Choose a conversation from the list to start messaging.
		</p>
	</div>
{:else}
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
	{@const displayName = authorQuery.data?.display_name || authorQuery.data?.name || npub.slice(0, 12)}
	{@const avatarUrl = authorQuery.data?.picture}

	<div class="flex flex-col h-full">
		<!-- Header -->
		<div
			class="flex items-center gap-3 p-4 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60"
		>
			<img
				src={avatarUrl || `https://api.dicebear.com/7.x/identicon/svg?seed=${conversation.pubkey}`}
				alt={displayName}
				class="h-10 w-10 rounded-full"
			/>
			<div>
				<h2 class="font-semibold">{displayName}</h2>
				{#if authorQuery.data?.nip05}
					<p class="text-xs text-muted-foreground">{authorQuery.data.nip05}</p>
				{/if}
			</div>
		</div>

		<!-- Messages -->
		<ScrollArea.Root class="flex-1">
			<div bind:this={scrollViewportRef} class="h-full p-4">
				{#each conversation.messages as message (message.id)}
					{@const messageAuthorQuery = createQuery(() => ({
						queryKey: ['profile', message.pubkey],
						queryFn: async ({ signal }) => {
							const events = await loadWithRouter({
								filters: [{ kinds: [0], authors: [message.pubkey], limit: 1 }],
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
					{@const authorNpub = nip19.npubEncode(message.pubkey)}
					{@const messageDisplayName =
						messageAuthorQuery.data?.display_name ||
						messageAuthorQuery.data?.name ||
						authorNpub.slice(0, 12)}
					{@const messageAvatarUrl = messageAuthorQuery.data?.picture}
					{@const timeAgo = formatTimestamp(message.created_at)}

					<div
						class={cn('flex gap-3 mb-4', message.isSent ? 'flex-row-reverse' : 'flex-row')}
					>
						<img
							src={messageAvatarUrl ||
								`https://api.dicebear.com/7.x/identicon/svg?seed=${message.pubkey}`}
							alt={messageDisplayName}
							class="h-8 w-8 rounded-full flex-shrink-0"
						/>

						<div
							class={cn(
								'flex flex-col gap-1 max-w-[70%]',
								message.isSent ? 'items-end' : 'items-start'
							)}
						>
							<div
								class={cn(
									'rounded-2xl px-4 py-2 break-words',
									message.isSent
										? 'bg-primary text-primary-foreground'
										: 'bg-muted text-foreground'
								)}
							>
								<p class="text-sm whitespace-pre-wrap">{message.content || '[Unable to decrypt message]'}</p>
							</div>
							<span class="text-xs text-muted-foreground px-2">{timeAgo}</span>
						</div>
					</div>
				{/each}
			</div>
			<ScrollArea.Scrollbar orientation="vertical" />
		</ScrollArea.Root>

		<!-- Input -->
		<div
			class="p-4 border-t bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60"
		>
			<div class="flex gap-2">
				<Textarea
					bind:this={textareaRef}
					bind:value={messageText}
					onkeydown={handleKeyDown}
					placeholder="Type a message..."
					class="min-h-[60px] max-h-[200px] resize-none"
					disabled={sendMessageMutation.isPending}
				/>
				<Button
					size="icon"
					onclick={handleSendMessage}
					disabled={!messageText.trim() || sendMessageMutation.isPending}
					class="h-[60px] w-[60px] flex-shrink-0"
				>
					<Send class="h-5 w-5" />
				</Button>
			</div>
			<p class="text-xs text-muted-foreground mt-2">
				Press Enter to send, Shift+Enter for new line
			</p>
		</div>
	</div>
{/if}
