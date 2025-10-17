<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<MenubarPrimitive.Root
  class={cn('flex h-10 items-center space-x-1 rounded-md border bg-background p-1', className)}
>
  {#if children}
    {@render children()}
  {/if}
</MenubarPrimitive.Root>
