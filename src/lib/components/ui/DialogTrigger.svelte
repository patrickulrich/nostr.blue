<script lang="ts">
  import { Dialog as DialogPrimitive } from 'bits-ui';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();
</script>

<DialogPrimitive.Trigger>
  {#if children}
    {@render children()}
  {/if}
</DialogPrimitive.Trigger>
