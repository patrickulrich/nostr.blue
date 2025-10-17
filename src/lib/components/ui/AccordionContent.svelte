<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';
  import type { Writable } from 'svelte/store';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();

  const { openItems } = getContext<{
    openItems: Writable<string[]>;
  }>('accordion');

  const { value } = getContext<{ value: string }>('accordion-item');

  const isOpen = $derived($openItems.includes(value));
</script>

{#if isOpen}
  <div
    class="overflow-hidden text-sm transition-all data-[state=closed]:animate-accordion-up data-[state=open]:animate-accordion-down"
    data-state={isOpen ? 'open' : 'closed'}
  >
    <div class={cn('pb-4 pt-0', className)}>
      {#if children}
        {@render children()}
      {/if}
    </div>
  </div>
{/if}
