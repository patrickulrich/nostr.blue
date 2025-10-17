<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';

  interface Props {
    value?: string;
    onValueChange?: (value: string) => void;
    children?: import('svelte').Snippet;
  }

  let { value = $bindable(''), onValueChange, children }: Props = $props();
</script>

<ContextMenuPrimitive.RadioGroup bind:value {onValueChange}>
  {#if children}
    {@render children()}
  {/if}
</ContextMenuPrimitive.RadioGroup>
