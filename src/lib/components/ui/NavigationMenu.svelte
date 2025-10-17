<script lang="ts">
  import { NavigationMenu as NavigationMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import NavigationMenuViewport from './NavigationMenuViewport.svelte';

  interface Props {
    value?: string;
    onValueChange?: (value: string) => void;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { value = $bindable(''), onValueChange, class: className, children }: Props = $props();
</script>

<NavigationMenuPrimitive.Root
  bind:value
  {onValueChange}
  class={cn('relative z-10 flex max-w-max flex-1 items-center justify-center', className)}
>
  {#if children}
    {@render children()}
  {/if}
  <NavigationMenuViewport />
</NavigationMenuPrimitive.Root>
