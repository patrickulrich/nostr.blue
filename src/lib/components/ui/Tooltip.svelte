<script lang="ts">
  import { Tooltip as TooltipPrimitive } from 'bits-ui';

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    children?: import('svelte').Snippet;
  }

  let { open, onOpenChange, children }: Props = $props();
</script>

<TooltipPrimitive.Root {open} {onOpenChange}>
  {#if children}
    {@render children()}
  {/if}
</TooltipPrimitive.Root>
