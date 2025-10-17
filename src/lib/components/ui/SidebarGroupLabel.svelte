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
  data-sidebar="group-label"
  class={cn(
    'duration-200 flex h-8 shrink-0 items-center rounded-md px-2 text-xs font-medium text-sidebar-foreground/70 outline-none ring-sidebar-ring transition-[margin,opacity] ease-linear focus-visible:ring-2 [&>svg]:size-4 [&>svg]:shrink-0',
    'group-data-[collapsible=icon]:-mt-8 group-data-[collapsible=icon]:opacity-0',
    className
  )}
  {...restProps}
>
  {#if children}
    {@render children()}
  {/if}
</div>
