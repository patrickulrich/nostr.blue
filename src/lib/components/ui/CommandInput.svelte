<script lang="ts">
  import { Command as CommandPrimitive } from 'cmdk-sv';
  import { cn } from '$lib/utils';

  interface Props {
    value?: string;
    placeholder?: string;
    class?: string;
  }

  let { value = $bindable(''), placeholder, class: className }: Props = $props();
</script>

<div class="flex items-center border-b px-3" data-cmdk-input-wrapper="">
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    stroke-width="2"
    stroke-linecap="round"
    stroke-linejoin="round"
    class="mr-2 h-4 w-4 shrink-0 opacity-50"
  >
    <circle cx="11" cy="11" r="8" />
    <path d="m21 21-4.3-4.3" />
  </svg>
  <CommandPrimitive.Input
    bind:value
    {placeholder}
    class={cn(
      'flex h-11 w-full rounded-md bg-transparent py-3 text-sm outline-none placeholder:text-muted-foreground disabled:cursor-not-allowed disabled:opacity-50',
      className
    )}
  />
</div>
