<script lang="ts">
  import { Select as SelectPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { class: className, children }: Props = $props();
</script>

<SelectPrimitive.GroupHeading class={cn('py-1.5 pl-8 pr-2 text-sm font-semibold', className)}>
  {#if children}
    {@render children()}
  {/if}
</SelectPrimitive.GroupHeading>
