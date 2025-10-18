<script lang="ts">
	import { useNostrPublish } from '$lib/stores/publish.svelte';
	import { useToast } from '$lib/stores/toast.svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { Textarea } from '$lib/components/ui/textarea';
	import { currentUser } from '$lib/stores/auth';

	interface Props {
		isOpen: boolean;
		onClose: () => void;
		replyTo?: string; // Event ID to reply to
		class?: string;
	}

	let { isOpen = $bindable(), onClose, replyTo, class: className = '' }: Props = $props();

	// State
	let content = $state('');

	// Publish mutation
	const publish = useNostrPublish();
	const toast = useToast();

	// Character counter
	let charCount = $derived(content.length);
	let isOverLimit = $derived(charCount > 5000);

	// Handle submit
	async function handleSubmit() {
		if (!content.trim() || isOverLimit) return;
		if (!$currentUser) {
			toast.toastError('Please log in to publish notes');
			return;
		}

		const tags: string[][] = [];

		// Add reply tags if replying to another note
		if (replyTo) {
			tags.push(['e', replyTo, '', 'reply']);
		}

		try {
			await publish.mutateAsync({
				kind: 1,
				content: content.trim(),
				tags
			});

			toast.toastSuccess('Note published successfully!');
			content = '';
			onClose();
		} catch (error) {
			toast.toastError('Failed to publish note');
			console.error('Publish error:', error);
		}
	}

	// Handle cancel
	function handleCancel() {
		content = '';
		onClose();
	}
</script>

<Dialog.Root bind:open={isOpen}>
	<Dialog.Content class="sm:max-w-[600px] {className}">
		<Dialog.Header>
			<Dialog.Title>{replyTo ? 'Reply to Note' : 'New Note'}</Dialog.Title>
			<Dialog.Description>
				{replyTo
					? 'Share your thoughts in response to this note.'
					: 'Share your thoughts with the Nostr network.'}
			</Dialog.Description>
		</Dialog.Header>

		<div class="space-y-4">
			<!-- Text area -->
			<Textarea
				bind:value={content}
				placeholder="What's on your mind?"
				class="min-h-[200px] resize-none"
				disabled={publish.isPending}
			/>

			<!-- Character counter -->
			<div class="flex justify-between items-center text-sm">
				<span class="text-muted-foreground">
					{#if isOverLimit}
						<span class="text-destructive font-medium">
							{charCount} / 5000 characters (limit exceeded)
						</span>
					{:else}
						{charCount} / 5000 characters
					{/if}
				</span>
			</div>
		</div>

		<Dialog.Footer class="gap-2">
			<Button variant="outline" onclick={handleCancel} disabled={publish.isPending}>
				Cancel
			</Button>
			<Button
				onclick={handleSubmit}
				disabled={!content.trim() || isOverLimit || publish.isPending || !$currentUser}
			>
				{#if publish.isPending}
					Publishing...
				{:else}
					{replyTo ? 'Reply' : 'Publish'}
				{/if}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
