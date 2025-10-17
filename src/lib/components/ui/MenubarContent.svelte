<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    align?: 'start' | 'center' | 'end';
    alignOffset?: number;
    sideOffset?: number;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    align = 'start',
    alignOffset = -4,
    sideOffset = 8,
    class: className,
    children
  }: Props = $props();
</script>

<MenubarPrimitive.Portal>
  <MenubarPrimitive.Content
    {align}
    {alignOffset}
    {sideOffset}
    class={cn(
      'z-50 min-w-[12rem] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md data-[state=open]:animate-in data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2',
      className
    )}
  >
    {#if children}
      {@render children()}
    {/if}
  </MenubarPrimitive.Content>
</MenubarPrimitive.Portal>
