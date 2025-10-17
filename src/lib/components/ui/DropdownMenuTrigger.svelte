<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();
</script>

<DropdownMenuPrimitive.Trigger>
  {#if children}
    {@render children()}
  {/if}
</DropdownMenuPrimitive.Trigger>
