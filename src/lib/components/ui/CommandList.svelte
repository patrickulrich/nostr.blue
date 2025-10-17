<script lang="ts">
  import { Command as CommandPrimitive } from 'cmdk-sv';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<CommandPrimitive.List class={cn('max-h-[300px] overflow-y-auto overflow-x-hidden', className)}>
  {#if children}
    {@render children()}
  {/if}
</CommandPrimitive.List>
