<script lang="ts">
  import { Collapsible as CollapsiblePrimitive } from 'bits-ui';

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    disabled?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    open = $bindable(false),
    onOpenChange,
    disabled = false,
    class: className,
    children
  }: Props = $props();
</script>

<CollapsiblePrimitive.Root bind:open {onOpenChange} {disabled} class={className}>
  {#if children}
    {@render children()}
  {/if}
</CollapsiblePrimitive.Root>
