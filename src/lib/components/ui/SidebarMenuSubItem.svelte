<script lang="ts">
  /* eslint-disable svelte/valid-compile */
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends HTMLAttributes<HTMLLIElement> {
    children?: import('svelte').Snippet;
  }

  let { children, ...restProps }: Props = $props();
</script>

<li {...restProps}>
  {#if children}
    {@render children()}
  {/if}
</li>
