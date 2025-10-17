<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import { flyAndScale } from '$lib/transitions';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<DropdownMenuPrimitive.SubContent
  forceMount
  class={cn(
    'z-50 min-w-[8rem] overflow-hidden rounded-md border bg-popover p-1 text-popover-foreground shadow-lg',
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
</DropdownMenuPrimitive.SubContent>
