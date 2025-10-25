<script lang="ts">
	import { cn } from '$lib/utils';
	import type { Snippet } from 'svelte';

	interface Props {
		sidebar?: Snippet;
		rightPanel?: Snippet;
		children?: Snippet;
		class?: string;
	}

	let { sidebar, rightPanel, children, class: className }: Props = $props();
</script>

<div class="min-h-screen bg-background">
	<div class="max-w-[1280px] mx-auto flex">
		<!-- Left Sidebar - Navigation -->
		{#if sidebar}
			<aside
				class="hidden lg:flex lg:w-[275px] flex-shrink-0 sticky top-0 h-screen border-r border-border"
			>
				<div class="flex flex-col w-full p-4">
					{@render sidebar()}
				</div>
			</aside>
		{/if}

		<!-- Main Content -->
		<main class={cn('flex-1 min-w-0 border-r border-border', className)}>
			{#if children}
				{@render children()}
			{/if}
		</main>

		<!-- Right Panel - Trends, Suggestions, etc. -->
		{#if rightPanel}
			<aside class="hidden xl:flex xl:w-[350px] flex-shrink-0 sticky top-0 h-screen">
				<div class="flex flex-col w-full p-4">
					{@render rightPanel()}
				</div>
			</aside>
		{/if}
	</div>
</div>
