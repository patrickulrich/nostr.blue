<script lang="ts">
  import { Drawer as DrawerPrimitive } from 'vaul-svelte';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();
</script>

<DrawerPrimitive.Trigger>
  {#if children}
    {@render children()}
  {/if}
</DrawerPrimitive.Trigger>
