<script lang="ts">
  import { Dialog as SheetPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<SheetPrimitive.Trigger class={className}>
  {#if children}
    {@render children()}
  {/if}
</SheetPrimitive.Trigger>
