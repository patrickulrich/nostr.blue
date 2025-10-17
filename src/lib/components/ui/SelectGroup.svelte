<script lang="ts">
  import { Select as SelectPrimitive } from 'bits-ui';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();
</script>

<SelectPrimitive.Group>
  {#if children}
    {@render children()}
  {/if}
</SelectPrimitive.Group>
