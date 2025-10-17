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

<div
  role="group"
  aria-roledescription="slide"
  class={cn(
    'min-w-0 shrink-0 grow-0 basis-full',
    orientation === 'horizontal' ? 'pl-4' : 'pt-4',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</div>
