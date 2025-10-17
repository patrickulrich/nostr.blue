<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';
  import { buttonVariants } from './button-variants';
  import type { Writable } from 'svelte/store';

  interface Props {
    class?: string;
    onclick?: () => void;
    children?: import('svelte').Snippet;
  }

  let { class: className, onclick, children }: Props = $props();

  const { close } = getContext<{
    isOpen: Writable<boolean>;
    close: () => void;
  }>('alert-dialog');

  function handleClick() {
    onclick?.();
    close();
  }
</script>

<button type="button" class={cn(buttonVariants(), className)} onclick={handleClick}>
  {#if children}
    {@render children()}
  {/if}
</button>
