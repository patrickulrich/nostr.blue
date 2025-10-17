<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    disabled?: boolean;
    inset?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { disabled = false, inset = false, class: className, children }: Props = $props();
</script>

<MenubarPrimitive.Item
  {disabled}
  class={cn(
    'relative flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
    inset && 'pl-8',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</MenubarPrimitive.Item>
