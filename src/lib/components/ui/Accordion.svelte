<script lang="ts">
  import { setContext } from 'svelte';
  import { writable } from 'svelte/store';
  import { cn } from '$lib/utils';

  interface Props {
    type?: 'single' | 'multiple';
    value?: string | string[];
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { type = 'single', value = $bindable(type === 'single' ? '' : []), class: className, children }: Props = $props();

  const openItems = writable<string[]>(
    type === 'single'
      ? (value ? [value as string] : [])
      : (value as string[] || [])
  );

  function toggleItem(itemValue: string) {
    openItems.update(items => {
      if (type === 'single') {
        // Single mode: only one item can be open
        if (items.includes(itemValue)) {
          value = '';
          return [];
        } else {
          value = itemValue;
          return [itemValue];
        }
      } else {
        // Multiple mode: multiple items can be open
        if (items.includes(itemValue)) {
          const newItems = items.filter(v => v !== itemValue);
          value = newItems;
          return newItems;
        } else {
          const newItems = [...items, itemValue];
          value = newItems;
          return newItems;
        }
      }
    });
  }

  setContext('accordion', {
    openItems,
    toggleItem
  });
</script>

<div class={cn('', className)}>
  {#if children}
    {@render children()}
  {/if}
</div>
