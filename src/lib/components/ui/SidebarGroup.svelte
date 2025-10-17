<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'class'> {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children, ...restProps }: Props = $props();
</script>

<div
  data-sidebar="group"
  class={cn('relative flex w-full min-w-0 flex-col p-2', className)}
  {...restProps}
>
  {#if children}
    {@render children()}
  {/if}
</div>
