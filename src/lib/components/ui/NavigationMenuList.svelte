<script lang="ts">
  import { NavigationMenu as NavigationMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<NavigationMenuPrimitive.List
  class={cn('group flex flex-1 list-none items-center justify-center space-x-1', className)}
>
  {#if children}
    {@render children()}
  {/if}
</NavigationMenuPrimitive.List>
