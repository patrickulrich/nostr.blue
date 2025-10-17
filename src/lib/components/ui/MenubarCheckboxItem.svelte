<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    disabled?: boolean;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    checked = $bindable(false),
    onCheckedChange,
    disabled = false,
    class: className,
    children
  }: Props = $props();
</script>

<MenubarPrimitive.CheckboxItem
  bind:checked
  {onCheckedChange}
  {disabled}
  class={cn(
    'relative flex cursor-default select-none items-center rounded-sm py-1.5 pl-8 pr-2 text-sm outline-none focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50',
    className
  )}
>
  {#snippet child({ props, checked: isChecked })}
    <span class="absolute left-2 flex h-3.5 w-3.5 items-center justify-center">
      {#if isChecked}
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
    <div {...props}>
      {#if children}
        {@render children()}
      {/if}
    </div>
  {/snippet}
</MenubarPrimitive.CheckboxItem>
