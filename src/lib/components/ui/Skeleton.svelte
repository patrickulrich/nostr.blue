<script lang="ts">
  /* eslint-disable svelte/valid-compile */
  import { cn } from '$lib/utils';
  import type { HTMLAttributes } from 'svelte/elements';

  interface Props extends Omit<HTMLAttributes<HTMLDivElement>, 'class'> {
    class?: string;
  }

  let { class: className, ...restProps }: Props = $props();
</script>

<div class={cn('animate-pulse rounded-md bg-muted', className)} {...restProps}></div>
