<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    ratio?: number;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { ratio = 1, class: className, children }: Props = $props();

  const paddingBottom = $derived(`${100 / ratio}%`);
</script>

<div class={cn('relative w-full', className)}>
  <div style="padding-bottom: {paddingBottom};" class="relative w-full">
    <div class="absolute inset-0">
      {#if children}
        {@render children()}
      {/if}
    </div>
  </div>
</div>
