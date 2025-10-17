<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLElement>, 'class'> {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children, ...restProps }: Props = $props();
</script>

<nav
  aria-label="pagination"
  class={cn('mx-auto flex w-full justify-center', className)}
  {...restProps}
>
  {#if children}
    {@render children()}
  {/if}
</nav>
