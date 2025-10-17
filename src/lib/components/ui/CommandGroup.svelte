<script lang="ts">
  import { Command as CommandPrimitive } from 'cmdk-sv';
  import { cn } from '$lib/utils';

  interface Props {
    heading?: string;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { heading, class: className, children }: Props = $props();
</script>

<CommandPrimitive.Group
  {heading}
  class={cn(
    'overflow-hidden p-1 text-foreground [&_[cmdk-group-heading]]:px-2 [&_[cmdk-group-heading]]:py-1.5 [&_[cmdk-group-heading]]:text-xs [&_[cmdk-group-heading]]:font-medium [&_[cmdk-group-heading]]:text-muted-foreground',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</CommandPrimitive.Group>
