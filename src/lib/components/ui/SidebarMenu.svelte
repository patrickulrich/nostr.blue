<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLUListElement>, 'class'> {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children, ...restProps }: Props = $props();
</script>

<ul data-sidebar="menu" class={cn('flex w-full min-w-0 flex-col gap-1', className)} {...restProps}>
  {#if children}
    {@render children()}
  {/if}
</ul>
