<script lang="ts">
	import { useNostrPublish } from '$lib/stores/publish.svelte';
	import { useToast } from '$lib/stores/toast.svelte';
	import { Button } from '$lib/components/ui/button';
	import { Textarea } from '$lib/components/ui/textarea';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import { currentUser } from '$lib/stores/auth';
	import { createQuery, useQueryClient } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import { genUserName } from '$lib/genUserName';
	import { Loader2 } from 'lucide-svelte';

	interface Props {
		replyToEventId: string;
		replyToAuthor?: string;
		onSuccess?: () => void;
		class?: string;
	}

	let { replyToEventId, replyToAuthor, onSuccess, class: className = '' }: Props = $props();

	// State
	let content = $state('');
	let isFocused = $state(false);

	// Publish mutation
	const publish = useNostrPublish();
	const toast = useToast();
	const queryClient = useQueryClient();

	// Fetch current user's profile for avatar
	const profileQuery = createQuery(() => ({
		queryKey: ['profile', $currentUser?.pubkey],
		queryFn: async ({ signal }) => {
			if (!$currentUser) return null;

			const profiles = await loadWithRouter({
				filters: [{ kinds: [0], authors: [$currentUser.pubkey], limit: 1 }],
				signal
			});

			if (profiles[0]) {
				return JSON.parse(profiles[0].content);
			}
			return null;
		},
		enabled: !!$currentUser,
		staleTime: 60000
	}));

	const profile = $derived(profileQuery.data);
	const displayName = $derived(
		profile?.display_name || profile?.name || genUserName($currentUser?.pubkey || '')
	);
	const avatarUrl = $derived(profile?.picture);

	// Character counter
	let charCount = $derived(content.length);
	let isOverLimit = $derived(charCount > 5000);

	// Handle submit
	async function handleSubmit() {
		if (!content.trim() || isOverLimit) return;
		if (!$currentUser) {
			toast.toastError('Please log in to reply');
			return;
		}

		const tags: string[][] = [['e', replyToEventId, '', 'reply']];

		// Add mention of the author if provided
		if (replyToAuthor) {
			tags.push(['p', replyToAuthor]);
		}

		try {
			await publish.mutateAsync({
				kind: 1,
				content: content.trim(),
				tags
			});

			toast.toastSuccess('Reply published!');
			content = '';
			isFocused = false;

			// Invalidate replies query to refresh the thread
			queryClient.invalidateQueries({ queryKey: ['replies', replyToEventId] });

			onSuccess?.();
		} catch (error) {
			toast.toastError('Failed to publish reply');
			console.error('Publish error:', error);
		}
	}

	// Handle key events
	function handleKeydown(e: KeyboardEvent) {
		// Submit with Cmd/Ctrl + Enter
		if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
			e.preventDefault();
			handleSubmit();
		}
	}
</script>

{#if !$currentUser}
	<div class="rounded-lg border border-border p-6 text-center">
		<p class="text-muted-foreground">
			<a href="/" class="text-blue-500 hover:underline">Log in</a> to reply to this post
		</p>
	</div>
{:else}
	<div class="flex gap-3 {className}">
		<!-- User avatar -->
		<Avatar class="w-10 h-10 flex-shrink-0">
			<AvatarImage src={avatarUrl} alt={displayName} />
			<AvatarFallback>{displayName[0]?.toUpperCase() || 'U'}</AvatarFallback>
		</Avatar>

		<!-- Composer area -->
		<div class="flex-1 space-y-3">
			<Textarea
				bind:value={content}
				placeholder="Post your reply"
				class="min-h-[100px] resize-none"
				onfocus={() => (isFocused = true)}
				onkeydown={handleKeydown}
				disabled={publish.isPending}
			/>

			<!-- Actions row -->
			{#if isFocused || content.trim()}
				<div class="flex justify-between items-center">
					<!-- Character counter -->
					<div class="text-sm">
						{#if isOverLimit}
							<span class="text-destructive font-medium">
								{charCount} / 5000 characters (limit exceeded)
							</span>
						{:else}
							<span class="text-muted-foreground">{charCount} / 5000</span>
						{/if}
					</div>

					<!-- Action buttons -->
					<div class="flex gap-2">
						{#if content.trim()}
							<Button
								variant="ghost"
								size="sm"
								onclick={() => {
									content = '';
									isFocused = false;
								}}
								disabled={publish.isPending}
							>
								Cancel
							</Button>
						{/if}
						<Button
							size="sm"
							onclick={handleSubmit}
							disabled={!content.trim() || isOverLimit || publish.isPending}
						>
							{#if publish.isPending}
								<Loader2 class="h-4 w-4 mr-2 animate-spin" />
								Replying...
							{:else}
								Reply
							{/if}
						</Button>
					</div>
				</div>
			{:else}
				<!-- Hint when not focused -->
				<p class="text-xs text-muted-foreground">
					Tip: Press <kbd class="px-1.5 py-0.5 rounded bg-muted font-mono text-xs">Cmd</kbd> +{' '}
					<kbd class="px-1.5 py-0.5 rounded bg-muted font-mono text-xs">Enter</kbd> to reply
				</p>
			{/if}
		</div>
	</div>
{/if}
