<script lang="ts">
  import { Select as SelectPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    value: string;
    label?: string;
    disabled?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { value, label, disabled = false, class: className, children }: Props = $props();
</script>

<SelectPrimitive.Item
  {value}
  {label}
  {disabled}
  class={cn(
    'relative flex w-full cursor-default select-none items-center rounded-sm py-1.5 pl-8 pr-2 text-sm outline-none focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
    className
  )}
>
  {#snippet child({ props, selected })}
    <span class="absolute left-2 flex h-3.5 w-3.5 items-center justify-center">
      {#if selected}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          stroke-width="2"
          stroke-linecap="round"
          stroke-linejoin="round"
          class="h-4 w-4"
        >
          <path d="M20 6 9 17l-5-5" />
        </svg>
      {/if}
    </span>
    <span {...props}>
      {#if children}
        {@render children()}
      {/if}
    </span>
  {/snippet}
</SelectPrimitive.Item>
