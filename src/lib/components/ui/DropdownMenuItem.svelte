<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    inset?: boolean;
    disabled?: boolean;
    onclick?: () => void;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { inset = false, disabled = false, onclick, class: className, children }: Props = $props();
</script>

<DropdownMenuPrimitive.Item
  {disabled}
  {onclick}
  class={cn(
    'relative flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
    inset && 'pl-8',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</DropdownMenuPrimitive.Item>
