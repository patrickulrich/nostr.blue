<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import ConversationList from '$lib/components/ConversationList.svelte';
	import ConversationThread from '$lib/components/ConversationThread.svelte';
	import { Button } from '$lib/components/ui/button';
	import * as Card from '$lib/components/ui/card';
	import * as Separator from '$lib/components/ui/separator';
	import { Loader2, RefreshCw, MessageCircle } from 'lucide-svelte';
	import { currentUser, signer } from '$lib/stores/auth';
	import { createQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import {
		decryptMessage,
		groupMessagesIntoConversations,
		type DecryptedMessage
	} from '$lib/stores/messages.svelte';

	let selectedPubkey = $state<string | undefined>(undefined);

	// Fetch and decrypt direct messages
	const messagesQuery = createQuery(() => ({
		queryKey: ['direct-messages', $currentUser?.pubkey, $signer],
		queryFn: async ({ signal }) => {
			if (!$currentUser?.pubkey || !$signer) {
				console.log('No current user or signer', { user: $currentUser, signer: $signer });
				return [];
			}

			const userPubkey = $currentUser.pubkey;
			console.log('Fetching messages for pubkey:', userPubkey);

			// Query both sent and received kind 4 messages
			const events = await loadWithRouter({
				filters: [
					// Received messages
					{ kinds: [4], '#p': [userPubkey], limit: 500 },
					// Sent messages
					{ kinds: [4], authors: [userPubkey], limit: 500 }
				],
				signal,
			});

			console.log('Fetched events:', events.length);

			// Decrypt messages
			const decryptedMessages: DecryptedMessage[] = [];

			for (const event of events) {
				try {
					// Determine the other party in the conversation
					const isSent = event.pubkey === userPubkey;
					const otherPubkey = isSent
						? event.tags.find((t) => t[0] === 'p')?.[1]
						: event.pubkey;

					if (!otherPubkey) {
						console.log('No otherPubkey for event', event.id);
						continue;
					}

					// Decrypt the content
					let decryptedContent: string | undefined;
					try {
						decryptedContent = await decryptMessage(
							$signer,
							otherPubkey,
							event.content
						);
						console.log('Decrypted message from', otherPubkey.slice(0, 8));
					} catch (decryptError) {
						console.error('Failed to decrypt message:', decryptError);
						// Skip messages that fail to decrypt
						continue;
					}

					// Safety check - skip if decryption returned undefined
					if (!decryptedContent) {
						console.warn('Decryption returned empty content for event', event.id);
						continue;
					}

					decryptedMessages.push({
						id: event.id,
						pubkey: event.pubkey,
						otherPubkey,
						content: decryptedContent,
						created_at: event.created_at,
						isSent,
						rawEvent: event
					});
				} catch (error) {
					console.error('Error processing message:', error);
				}
			}

			console.log('Decrypted messages:', decryptedMessages.length);

			// Group into conversations
			const conversations = groupMessagesIntoConversations(decryptedMessages);
			console.log('Conversations:', conversations.length);
			return conversations;
		},
		enabled: !!$currentUser?.pubkey && !!$signer,
		staleTime: 30000, // 30 seconds
		refetchInterval: 60000 // Refetch every minute
	}));

	let conversations = $derived(messagesQuery.data || []);
	let isLoading = $derived(messagesQuery.isLoading);
	let isRefetching = $derived(messagesQuery.isRefetching);
	let selectedConversation = $derived(conversations.find((c) => c.pubkey === selectedPubkey));

	function handleSelectConversation(pubkey: string) {
		selectedPubkey = pubkey;
	}
</script>

<MainLayout>
	{#snippet sidebar()}
		<AppSidebar />
	{/snippet}

	{#snippet children()}
		<div class="min-h-screen">
			<!-- Not logged in -->
			{#if !$currentUser}
				<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
					<MessageCircle class="h-16 w-16 text-muted-foreground mb-4" />
					<h2 class="text-2xl font-bold mb-2">Private Messages</h2>
					<p class="text-muted-foreground max-w-sm mb-6">
						Sign in to send and receive encrypted direct messages on Nostr.
					</p>
				</div>
			{:else}
				<!-- Header -->
				<div class="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
					<div class="flex items-center justify-between p-4">
						<h1 class="text-xl font-bold flex items-center gap-2">
							<MessageCircle class="h-5 w-5 text-blue-500" />
							Messages
						</h1>
						<Button
							variant="outline"
							size="sm"
							onclick={() => messagesQuery.refetch()}
							disabled={isRefetching}
						>
							{#if isRefetching}
								<Loader2 class="h-4 w-4 animate-spin mr-2" />
							{:else}
								<RefreshCw class="h-4 w-4 mr-2" />
							{/if}
							Refresh
						</Button>
					</div>
				</div>

				<!-- Two column layout -->
				<Card.Root class="h-[calc(100vh-8rem)] flex overflow-hidden border-x-0 border-b-0 rounded-none">
					<!-- Left: Conversation List -->
					<div class="w-80 border-r flex flex-col">
						{#if isLoading}
							<div class="flex items-center justify-center h-full">
								<Loader2 class="h-8 w-8 animate-spin text-muted-foreground" />
							</div>
						{:else}
							<ConversationList
								{conversations}
								{selectedPubkey}
								onSelectConversation={handleSelectConversation}
							/>
						{/if}
					</div>

					<Separator.Root orientation="vertical" />

					<!-- Right: Conversation Thread -->
					<div class="flex-1">
						<ConversationThread conversation={selectedConversation} />
					</div>
				</Card.Root>
			{/if}
		</div>
	{/snippet}
</MainLayout>
