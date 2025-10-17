<script lang="ts">
  import { Tooltip as TooltipPrimitive } from 'bits-ui';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<TooltipPrimitive.Trigger class={className}>
  {#if children}
    {@render children()}
  {/if}
</TooltipPrimitive.Trigger>
