<script lang="ts">
  import { setContext } from 'svelte';
  import { writable } from 'svelte/store';

  interface Props {
    name: string;
    error?: string;
    children?: import('svelte').Snippet;
  }

  let { name, error, children }: Props = $props();

  const fieldId = `field-${Math.random().toString(36).substr(2, 9)}`;
  const descriptionId = `${fieldId}-description`;
  const messageId = `${fieldId}-message`;

  setContext('formField', {
    name,
    fieldId,
    descriptionId,
    messageId,
    error: writable(error)
  });

  $effect(() => {
    const errorStore = writable(error);
    setContext('formField', {
      name,
      fieldId,
      descriptionId,
      messageId,
      error: errorStore
    });
  });
</script>

{#if children}
  {@render children()}
{/if}
