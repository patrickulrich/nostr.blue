<script lang="ts">
  /* eslint-disable svelte/valid-compile */
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLUListElement>, 'class'> {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children, ...restProps }: Props = $props();
</script>

<ul
  data-sidebar="menu-sub"
  class={cn(
    'mx-3.5 flex min-w-0 translate-x-px flex-col gap-1 border-l border-sidebar-border px-2.5 py-0.5',
    'group-data-[collapsible=icon]:hidden',
    className
  )}
  {...restProps}
>
  {#if children}
    {@render children()}
  {/if}
</ul>
