<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();

  const { orientation } = getContext<{ orientation: 'horizontal' | 'vertical' }>('carousel');
</script>

<div class="overflow-hidden">
  <div
    class={cn(
      'flex',
      orientation === 'horizontal' ? '-ml-4' : '-mt-4 flex-col',
      className
    )}
  >
    {#if children}
      {@render children()}
    {/if}
  </div>
</div>
