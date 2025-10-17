<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import { flyAndScale } from '$lib/transitions';

  interface Props {
    sideOffset?: number;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { sideOffset = 4, class: className, children }: Props = $props();
</script>

<DropdownMenuPrimitive.Portal>
  <DropdownMenuPrimitive.Content
    {sideOffset}
    forceMount
    class={cn(
      'z-50 min-w-[8rem] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-md',
      className
    )}
  >
    {#snippet child({ props, open })}
      {#if open}
        <div {...props} transition:flyAndScale>
          {#if children}
            {@render children()}
          {/if}
        </div>
      {/if}
    {/snippet}
  </DropdownMenuPrimitive.Content>
</DropdownMenuPrimitive.Portal>
