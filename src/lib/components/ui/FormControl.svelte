<script lang="ts">
  import { getContext } from 'svelte';
  import type { Writable } from 'svelte/store';

  interface Props {
    children?: import('svelte').Snippet;
  }

  let { children }: Props = $props();

  const context = getContext<{
    fieldId: string;
    descriptionId: string;
    messageId: string;
    error: Writable<string | undefined>;
  }>('formField');

  const fieldId = context?.fieldId;
  const descriptionId = context?.descriptionId;
  const messageId = context?.messageId;
  const error = context?.error;

  const describedBy = $derived(
    error && $error ? `${descriptionId} ${messageId}` : descriptionId
  );
  const invalid = $derived(!!(error && $error));
</script>

<div
  id={fieldId}
  aria-describedby={describedBy}
  aria-invalid={invalid}
>
  {#if children}
    {@render children()}
  {/if}
</div>
