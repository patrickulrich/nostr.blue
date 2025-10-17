<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';

  interface Props {
    open?: boolean;
    onOpenChange?: (open: boolean) => void;
    children?: import('svelte').Snippet;
  }

  let { open = $bindable(false), onOpenChange, children }: Props = $props();
</script>

<MenubarPrimitive.Sub bind:open {onOpenChange}>
  {#if children}
    {@render children()}
  {/if}
</MenubarPrimitive.Sub>
