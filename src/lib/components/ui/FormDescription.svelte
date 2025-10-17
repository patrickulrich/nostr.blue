<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();

  const context = getContext<{ descriptionId: string }>('formField');
  const descriptionId = context?.descriptionId;
</script>

<p id={descriptionId} class={cn('text-sm text-muted-foreground', className)}>
  {#if children}
    {@render children()}
  {/if}
</p>
