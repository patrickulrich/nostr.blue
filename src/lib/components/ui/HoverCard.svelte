<script lang="ts">
  import { LinkPreview } from 'bits-ui';

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    openDelay?: number;
    closeDelay?: number;
    children?: import('svelte').Snippet;
  }

  let {
    open = $bindable(false),
    onOpenChange,
    openDelay = 700,
    closeDelay = 300,
    children
  }: Props = $props();
</script>

<LinkPreview.Root bind:open {onOpenChange} {openDelay} {closeDelay}>
  {#if children}
    {@render children()}
  {/if}
</LinkPreview.Root>
