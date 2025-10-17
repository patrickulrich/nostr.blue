<script lang="ts">
  import { Dialog as SheetPrimitive } from 'bits-ui';

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    children?: import('svelte').Snippet;
  }

  let { open = $bindable(false), onOpenChange, children }: Props = $props();
</script>

<SheetPrimitive.Root bind:open {onOpenChange}>
  {#if children}
    {@render children()}
  {/if}
</SheetPrimitive.Root>
