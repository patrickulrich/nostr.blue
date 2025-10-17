<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();
</script>

<DropdownMenuPrimitive.Group>
  {#if children}
    {@render children()}
  {/if}
</DropdownMenuPrimitive.Group>
