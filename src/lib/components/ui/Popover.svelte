<script lang="ts">
  import { Popover as PopoverPrimitive } from 'bits-ui';

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    children?: import('svelte').Snippet;
  }

  let { open = $bindable(false), onOpenChange, children }: Props = $props();
</script>

<PopoverPrimitive.Root bind:open {onOpenChange}>
  {#if children}
    {@render children()}
  {/if}
</PopoverPrimitive.Root>
