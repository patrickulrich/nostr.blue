<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'class'> {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children, ...restProps }: Props = $props();
</script>

<div data-sidebar="footer" class={cn('flex flex-col gap-2 p-2', className)} {...restProps}>
  {#if children}
    {@render children()}
  {/if}
</div>
