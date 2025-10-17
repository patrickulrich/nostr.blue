<script lang="ts">
  import { setContext } from 'svelte';
  import { writable } from 'svelte/store';

  interface Props {
    open?: boolean;
    children?: import('svelte').Snippet;
  }

  let { open = $bindable(false), children }: Props = $props();

  const isOpen = writable(open);

  $effect(() => {
    isOpen.set(open);
  });

  $effect(() => {
    open = $isOpen;
  });

  function close() {
    open = false;
  }

  setContext('alert-dialog', {
    isOpen,
    close
  });
</script>

{#if children}
  {@render children()}
{/if}
