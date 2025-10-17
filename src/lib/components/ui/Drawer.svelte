<script lang="ts">
  import { Drawer as DrawerPrimitive } from 'vaul-svelte';

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    shouldScaleBackground?: boolean;
    children?: import('svelte').Snippet;
  }

  let {
    open = $bindable(false),
    onOpenChange,
    shouldScaleBackground = true,
    children
  }: Props = $props();
</script>

<DrawerPrimitive.Root bind:open {onOpenChange} {shouldScaleBackground}>
  {#if children}
    {@render children()}
  {/if}
</DrawerPrimitive.Root>
