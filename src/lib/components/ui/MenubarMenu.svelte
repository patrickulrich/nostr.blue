<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';

  interface Props {
    value?: string;
    onOpenChange?: (open: boolean) => void;
    children?: import('svelte').Snippet;
  }

  let { value, onOpenChange, children }: Props = $props();
</script>

<MenubarPrimitive.Menu {value} {onOpenChange}>
  {#if children}
    {@render children()}
  {/if}
</MenubarPrimitive.Menu>
