<script lang="ts">
  import { Drawer as DrawerPrimitive } from 'vaul-svelte';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<DrawerPrimitive.Portal>
  <DrawerPrimitive.Overlay class="fixed inset-0 z-50 bg-black/80" />
  <DrawerPrimitive.Content
    class={cn(
      'fixed inset-x-0 bottom-0 z-50 mt-24 flex h-auto flex-col rounded-t-[10px] border bg-background',
      className
    )}
  >
    <div class="mx-auto mt-4 h-2 w-[100px] rounded-full bg-muted"></div>
    {#if children}
      {@render children()}
    {/if}
  </DrawerPrimitive.Content>
</DrawerPrimitive.Portal>
