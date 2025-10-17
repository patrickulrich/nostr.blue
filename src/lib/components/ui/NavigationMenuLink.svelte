<script lang="ts">
  import { NavigationMenu as NavigationMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    href?: string;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { href, class: className, children }: Props = $props();
</script>

<NavigationMenuPrimitive.Link {href} class={className}>
  {#if children}
    {@render children()}
  {/if}
</NavigationMenuPrimitive.Link>
