<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    children?: import('svelte').Snippet;
  }

  let { open = $bindable(false), onOpenChange, children }: Props = $props();
</script>

<DropdownMenuPrimitive.Root bind:open {onOpenChange}>
  {#if children}
    {@render children()}
  {/if}
</DropdownMenuPrimitive.Root>
