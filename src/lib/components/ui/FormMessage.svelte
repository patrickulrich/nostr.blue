<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';
  import type { Writable } from 'svelte/store';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();

  const context = getContext<{
    messageId: string;
    error: Writable<string | undefined>;
  }>('formField');

  const messageId = context?.messageId;
  const error = context?.error;

  const body = $derived((error && $error) || children);
</script>

{#if body}
  <p id={messageId} class={cn('text-sm font-medium text-destructive', className)}>
    {#if error && $error}
      {$error}
    {:else if children}
      {@render children()}
    {/if}
  </p>
{/if}
