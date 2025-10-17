<script lang="ts">
  import { Tooltip as TooltipPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    sideOffset?: number;
    side?: 'top' | 'right' | 'bottom' | 'left';
    align?: 'start' | 'center' | 'end';
    alignOffset?: number;
    hidden?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    sideOffset = 4,
    side,
    align,
    alignOffset,
    hidden,
    class: className,
    children
  }: Props = $props();
</script>

<TooltipPrimitive.Portal>
  <TooltipPrimitive.Content
    {sideOffset}
    {side}
    {align}
    {alignOffset}
    {hidden}
    class={cn(
      'z-50 overflow-hidden rounded-md border bg-popover px-3 py-1.5 text-sm text-popover-foreground shadow-md animate-in fade-in-0 zoom-in-95 data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=closed]:zoom-out-95 data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2',
      className
    )}
  >
    {#if children}
      {@render children()}
    {/if}
  </TooltipPrimitive.Content>
</TooltipPrimitive.Portal>
