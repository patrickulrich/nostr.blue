<script lang="ts">
  import { Drawer as DrawerPrimitive } from 'vaul-svelte';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<DrawerPrimitive.Description class={cn('text-sm text-muted-foreground', className)}>
  {#if children}
    {@render children()}
  {/if}
</DrawerPrimitive.Description>
