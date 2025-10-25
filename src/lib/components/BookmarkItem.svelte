<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import { ExternalLink, Hash, FileText, X } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import Note from '$lib/components/Note.svelte';
	import { nip19 } from 'nostr-tools';
	import type { Bookmark } from '$lib/hooks/useUserBookmarks.svelte';
	import { cn } from '$lib/utils';

	interface Props {
		bookmark: Bookmark;
		onRemove?: () => void;
		class?: string;
	}

	let { bookmark, onRemove, class: className }: Props = $props();

	// Fetch the actual event for note bookmarks
	const noteQuery = createQuery(() => ({
		queryKey: ['bookmark-event', bookmark.value],
		queryFn: async ({ signal }) => {
			if (bookmark.type !== 'note') return null;

			const events = await loadWithRouter({
				filters: [{ ids: [bookmark.value], kinds: [1], limit: 1 }],
				signal
			});

			return events[0] || null;
		},
		enabled: bookmark.type === 'note',
		staleTime: 60000
	}));
</script>

{#if bookmark.type === 'note'}
	{#if noteQuery.isLoading}
		<!-- Loading skeleton -->
		<div class={cn('border-b border-border p-4 animate-pulse', className)}>
			<div class="h-20 bg-muted rounded"></div>
		</div>
	{:else if noteQuery.data}
		<!-- Note card with remove button -->
		<div class={cn('relative group', className)}>
			<Note event={noteQuery.data} />
			{#if onRemove}
				<Button
					variant="ghost"
					size="icon"
					onclick={(e) => {
						e.preventDefault();
						e.stopPropagation();
						onRemove();
					}}
					class="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity bg-background/80 hover:bg-destructive hover:text-destructive-foreground"
				>
					<X class="h-4 w-4" />
				</Button>
			{/if}
		</div>
	{/if}
{:else if bookmark.type === 'article'}
	{@const [kind, pubkey, identifier] = bookmark.value.split(':')}
	{@const naddr = nip19.naddrEncode({
		kind: parseInt(kind),
		pubkey,
		identifier
	})}
	<div class={cn('border-b border-border p-4 hover:bg-accent/50 transition-colors group', className)}>
		<a href="/{naddr}" class="flex items-start gap-3">
			<div class="flex-shrink-0 w-10 h-10 rounded-lg bg-blue-500/10 flex items-center justify-center">
				<FileText class="h-5 w-5 text-blue-500" />
			</div>
			<div class="flex-1 min-w-0">
				<div class="font-semibold mb-1">Article</div>
				<div class="text-sm text-muted-foreground truncate font-mono">
					{bookmark.value}
				</div>
			</div>
			{#if onRemove}
				<Button
					variant="ghost"
					size="icon"
					onclick={(e) => {
						e.preventDefault();
						e.stopPropagation();
						onRemove();
					}}
					class="opacity-0 group-hover:opacity-100 transition-opacity hover:bg-destructive hover:text-destructive-foreground"
				>
					<X class="h-4 w-4" />
				</Button>
			{/if}
		</a>
	</div>
{:else if bookmark.type === 'hashtag'}
	<div class={cn('border-b border-border p-4 hover:bg-accent/50 transition-colors group', className)}>
		<a href="/t/{bookmark.value}" class="flex items-start gap-3">
			<div class="flex-shrink-0 w-10 h-10 rounded-lg bg-blue-500/10 flex items-center justify-center">
				<Hash class="h-5 w-5 text-blue-500" />
			</div>
			<div class="flex-1 min-w-0">
				<div class="font-semibold mb-1">#{bookmark.value}</div>
				<div class="text-sm text-muted-foreground">Hashtag</div>
			</div>
			{#if onRemove}
				<Button
					variant="ghost"
					size="icon"
					onclick={(e) => {
						e.preventDefault();
						e.stopPropagation();
						onRemove();
					}}
					class="opacity-0 group-hover:opacity-100 transition-opacity hover:bg-destructive hover:text-destructive-foreground"
				>
					<X class="h-4 w-4" />
				</Button>
			{/if}
		</a>
	</div>
{:else if bookmark.type === 'url'}
	<div class={cn('border-b border-border p-4 hover:bg-accent/50 transition-colors group', className)}>
		<a href={bookmark.value} target="_blank" rel="noopener noreferrer" class="flex items-start gap-3">
			<div class="flex-shrink-0 w-10 h-10 rounded-lg bg-blue-500/10 flex items-center justify-center">
				<ExternalLink class="h-5 w-5 text-blue-500" />
			</div>
			<div class="flex-1 min-w-0">
				<div class="font-semibold mb-1 truncate">{bookmark.value}</div>
				<div class="text-sm text-muted-foreground">External Link</div>
			</div>
			{#if onRemove}
				<Button
					variant="ghost"
					size="icon"
					onclick={(e) => {
						e.preventDefault();
						e.stopPropagation();
						onRemove();
					}}
					class="opacity-0 group-hover:opacity-100 transition-opacity hover:bg-destructive hover:text-destructive-foreground"
				>
					<X class="h-4 w-4" />
				</Button>
			{/if}
		</a>
	</div>
{/if}
