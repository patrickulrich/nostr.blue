<script lang="ts">
  import { Command as CommandPrimitive } from 'cmdk-sv';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();
</script>

<CommandPrimitive.Empty class="py-6 text-center text-sm">
  {#if children}
    {@render children()}
  {:else}
    No results found.
  {/if}
</CommandPrimitive.Empty>
