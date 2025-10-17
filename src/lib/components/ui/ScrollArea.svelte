<script lang="ts">
  import { ScrollArea as ScrollAreaPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import ScrollBar from './ScrollBar.svelte';

  interface Props {
    type?: 'auto' | 'always' | 'scroll' | 'hover';
    dir?: 'ltr' | 'rtl';
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { type = 'hover', dir = 'ltr', class: className, children }: Props = $props();
</script>

<ScrollAreaPrimitive.Root {type} {dir} class={cn('relative overflow-hidden', className)}>
  <ScrollAreaPrimitive.Viewport class="h-full w-full rounded-[inherit]">
    {#if children}
      {@render children()}
    {/if}
  </ScrollAreaPrimitive.Viewport>
  <ScrollBar />
  <ScrollAreaPrimitive.Corner />
</ScrollAreaPrimitive.Root>
