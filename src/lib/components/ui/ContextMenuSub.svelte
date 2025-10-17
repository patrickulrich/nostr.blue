<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();
</script>

<ContextMenuPrimitive.Sub>
  {#if children}
    {@render children()}
  {/if}
</ContextMenuPrimitive.Sub>
