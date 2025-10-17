<script lang="ts">
  import { NavigationMenu as NavigationMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';
  import { navigationMenuTriggerStyle } from './navigation-menu-variants';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<NavigationMenuPrimitive.Trigger class={cn(navigationMenuTriggerStyle(), 'group', className)}>
  {#if children}
    {@render children()}
  {/if}
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    class="relative top-[1px] ml-1 h-3 w-3 transition duration-200 group-data-[state=open]:rotate-180"
    aria-hidden="true"
  >
    <path d="m6 9 6 6 6-6" />
  </svg>
</NavigationMenuPrimitive.Trigger>
