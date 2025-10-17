<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    inset?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { inset = false, class: className, children }: Props = $props();
</script>

<ContextMenuPrimitive.SubTrigger
  class={cn(
    'flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none focus:bg-accent focus:text-accent-foreground data-[state=open]:bg-accent data-[state=open]:text-accent-foreground',
    inset && 'pl-8',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    class="ml-auto h-4 w-4"
  >
    <path d="m9 18 6-6-6-6" />
  </svg>
</ContextMenuPrimitive.SubTrigger>
