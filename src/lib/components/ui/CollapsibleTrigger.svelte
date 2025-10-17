<script lang="ts">
  import { Collapsible as CollapsiblePrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<CollapsiblePrimitive.Trigger class={cn('', className)}>
  {#if children}
    {@render children()}
  {/if}
</CollapsiblePrimitive.Trigger>
