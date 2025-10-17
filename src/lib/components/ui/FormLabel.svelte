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
    fieldId: string;
    error: Writable<string | undefined>;
  }>('formField');

  const fieldId = context?.fieldId;
  const error = context?.error;
</script>

<label
  for={fieldId}
  class={cn(
    'text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70',
    error && $error && 'text-destructive',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</label>
