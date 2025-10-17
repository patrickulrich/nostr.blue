<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();
</script>

<MenubarPrimitive.Group>
  {#if children}
    {@render children()}
  {/if}
</MenubarPrimitive.Group>
