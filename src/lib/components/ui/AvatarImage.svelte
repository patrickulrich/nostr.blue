<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    src?: string;
    alt?: string;
    class?: string;
  }

  let { src, alt = '', class: className }: Props = $props();
</script>

{#if src}
  <img {src} {alt} class={cn('aspect-square h-full w-full object-cover', className)} />
{/if}
