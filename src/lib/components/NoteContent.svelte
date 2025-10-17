<script lang="ts">
	import type { TrustedEvent } from '@welshman/util';
	import { parse } from '@welshman/content';
	import { cn } from '$lib/utils';

	interface Props {
		event: TrustedEvent;
		class?: string;
	}

	let { event, class: className }: Props = $props();

	// Parse content using Welshman's parse for proper rendering
	let parsed = $derived(parse({ content: event.content, tags: event.tags }));
</script>

<div class={cn('whitespace-pre-wrap break-words', className)}>
	{#each parsed as part}
		{#if part.type === 'text'}
			{part.value}
		{:else if part.type === 'link'}
			<!-- @ts-expect-error - Welshman content types need refinement -->
			<a
				href={part.value}
				target="_blank"
				rel="noopener noreferrer"
				class="text-primary hover:underline"
			>
				{part.value}
			</a>
		{:else if part.type === 'invoice'}
			<code class="px-2 py-1 bg-muted rounded text-sm font-mono">
				{part.value}
			</code>
		{:else if part.type === 'code'}
			<code class="px-1 bg-muted rounded text-sm font-mono">
				{part.value}
			</code>
		{:else if part.type === 'event' || part.type === 'profile'}
			<!-- @ts-expect-error - Welshman content types need refinement -->
			<a href="/{part.value}" class="text-primary hover:underline font-medium">
				@{part.value.slice(0, 8)}...
			</a>
		{:else if part.type === 'topic'}
			<a href="/search?q={encodeURIComponent(part.value)}" class="text-primary hover:underline">
				#{part.value}
			</a>
		{/if}
	{/each}
</div>
