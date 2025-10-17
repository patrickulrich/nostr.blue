<script lang="ts">
  import { DropdownMenu as DropdownMenuPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    value: string;
    disabled?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { value, disabled = false, class: className, children }: Props = $props();
</script>

<DropdownMenuPrimitive.RadioItem
  {value}
  {disabled}
  class={cn(
    'relative flex cursor-default select-none items-center rounded-sm py-1.5 pl-8 pr-2 text-sm outline-none transition-colors focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
    className
  )}
>
  {#snippet child({ props, checked })}
    <span class="absolute left-2 flex h-3.5 w-3.5 items-center justify-center">
      {#if checked}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="currentColor"
          class="h-2 w-2"
        >
          <circle cx="12" cy="12" r="12" />
        </svg>
      {/if}
    </span>
    <div {...props}>
      {#if children}
        {@render children()}
      {/if}
    </div>
  {/snippet}
</DropdownMenuPrimitive.RadioItem>
