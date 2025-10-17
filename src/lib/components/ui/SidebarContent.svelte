<script lang="ts">
  /* eslint-disable svelte/valid-compile */
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'class'> {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children, ...restProps }: Props = $props();
</script>

<div
  data-sidebar="content"
  class={cn(
    'flex min-h-0 flex-1 flex-col gap-2 overflow-auto group-data-[collapsible=icon]:overflow-hidden',
    className
  )}
  {...restProps}
>
  {#if children}
    {@render children()}
  {/if}
</div>
