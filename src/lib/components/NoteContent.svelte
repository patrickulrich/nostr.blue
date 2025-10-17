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
			{@const url = typeof part.value === 'string' ? part.value : part.value.url}
			{@const hrefValue = typeof url === 'string' ? url : url.toString()}
			<a
				href={hrefValue}
				target="_blank"
				rel="noopener noreferrer"
				class="text-primary hover:underline"
			>
				{hrefValue}
			</a>
		{:else if part.type === 'invoice'}
			<code class="px-2 py-1 bg-muted rounded text-sm font-mono">
				{part.value}
			</code>
		{:else if part.type === 'code'}
			<code class="px-1 bg-muted rounded text-sm font-mono">
				{part.value}
			</code>
		{:else if part.type === 'event'}
			{@const eventValue = typeof part.value === 'object' && 'id' in part.value ? part.value.id : String(part.value)}
			<a href="/{eventValue}" class="text-primary hover:underline font-medium">
				@{eventValue.slice(0, 8)}...
			</a>
		{:else if part.type === 'profile'}
			{@const profileValue = typeof part.value === 'object' && 'pubkey' in part.value ? part.value.pubkey : String(part.value)}
			<a href="/{profileValue}" class="text-primary hover:underline font-medium">
				@{profileValue.slice(0, 8)}...
			</a>
		{:else if part.type === 'topic'}
			<a href="/search?q={encodeURIComponent(part.value)}" class="text-primary hover:underline">
				#{part.value}
			</a>
		{/if}
	{/each}
</div>
