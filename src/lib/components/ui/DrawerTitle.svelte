<script lang="ts">
  import { Drawer as DrawerPrimitive } from 'vaul-svelte';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<DrawerPrimitive.Title class={cn('text-lg font-semibold leading-none tracking-tight', className)}>
  {#if children}
    {@render children()}
  {/if}
</DrawerPrimitive.Title>
