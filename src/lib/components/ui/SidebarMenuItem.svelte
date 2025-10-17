<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLLIElement>, 'class'> {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children, ...restProps }: Props = $props();
</script>

<li
  data-sidebar="menu-item"
  class={cn('group/menu-item relative', className)}
  {...restProps}
>
  {#if children}
    {@render children()}
  {/if}
</li>
