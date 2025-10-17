<script lang="ts">
  import { NavigationMenu as NavigationMenuPrimitive } from 'bits-ui';

  interface Props {
    value?: string;
    children?: import('svelte').Snippet;
  }

  let { value, children }: Props = $props();
</script>

<NavigationMenuPrimitive.Item {value}>
  {#if children}
    {@render children()}
  {/if}
</NavigationMenuPrimitive.Item>
