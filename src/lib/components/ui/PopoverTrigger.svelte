<script lang="ts">
  import { Popover as PopoverPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<PopoverPrimitive.Trigger class={className}>
  {#if children}
    {@render children()}
  {/if}
</PopoverPrimitive.Trigger>
