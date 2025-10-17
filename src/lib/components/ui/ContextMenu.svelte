<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();
</script>

<ContextMenuPrimitive.Root>
  {#if children}
    {@render children()}
  {/if}
</ContextMenuPrimitive.Root>
