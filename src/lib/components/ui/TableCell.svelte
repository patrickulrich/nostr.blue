<script lang="ts">
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLTableCellElement>, 'class'> {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children, ...restProps }: Props = $props();
</script>

<td class={cn('p-4 align-middle [&:has([role=checkbox])]:pr-0', className)} {...restProps}>
  {#if children}
    {@render children()}
  {/if}
</td>
