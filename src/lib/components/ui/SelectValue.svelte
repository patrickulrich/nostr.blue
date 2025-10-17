<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    placeholder?: string;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { placeholder, class: className, children }: Props = $props();
</script>

<span class={cn('block truncate', className)}>
  {#if children}
    {@render children()}
  {:else if placeholder}
    <span class="text-muted-foreground">{placeholder}</span>
  {/if}
</span>
