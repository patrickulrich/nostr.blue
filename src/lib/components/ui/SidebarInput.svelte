<script lang="ts">
  import { cn } from '$lib/utils';
  import Input from './Input.svelte';

  interface Props {
    type?: string;
    value?: string;
    class?: string;
  }

  let { type = 'text', value = $bindable(''), class: className, ...restProps }: Props = $props();
</script>

<Input
  data-sidebar="input"
  {type}
  bind:value
  class={cn(
    'h-8 w-full bg-background shadow-none focus-visible:ring-2 focus-visible:ring-sidebar-ring',
    className
  )}
  {...restProps}
/>
