<script lang="ts">
  import { Command as CommandPrimitive } from 'cmdk-sv';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<CommandPrimitive.Root
  class={cn(
    'flex h-full w-full flex-col overflow-hidden rounded-md bg-popover text-popover-foreground',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</CommandPrimitive.Root>
