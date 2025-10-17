<script lang="ts">
  import { getContext } from 'svelte';
  import type { Writable } from 'svelte/store';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();

  const { isOpen } = getContext<{ isOpen: Writable<boolean> }>('alert-dialog');

  function open() {
    isOpen.set(true);
  }
</script>

<button type="button" onclick={open}>
  {#if children}
    {@render children()}
  {/if}
</button>
