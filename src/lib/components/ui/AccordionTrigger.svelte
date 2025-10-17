<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';
  import type { Writable } from 'svelte/store';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();

  const { openItems, toggleItem } = getContext<{
    openItems: Writable<string[]>;
    toggleItem: (value: string) => void;
  }>('accordion');

  const { value } = getContext<{ value: string }>('accordion-item');

  const isOpen = $derived($openItems.includes(value));
</script>

<div class="flex">
  <button
    type="button"
    class={cn(
      'flex flex-1 items-center justify-between py-4 font-medium transition-all hover:underline [&[data-state=open]>svg]:rotate-180',
      className
    )}
    data-state={isOpen ? 'open' : 'closed'}
    onclick={() => toggleItem(value)}
  >
    {#if children}
      {@render children()}
    {/if}
    <svg
      class="h-4 w-4 shrink-0 transition-transform duration-200"
      fill="none"
      stroke="currentColor"
      viewBox="0 0 24 24"
    >
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
    </svg>
  </button>
</div>
