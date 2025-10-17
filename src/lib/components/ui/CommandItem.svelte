<script lang="ts">
  import { Command as CommandPrimitive } from 'cmdk-sv';
  import { cn } from '$lib/utils';

  interface Props {
    value?: string;
    disabled?: boolean;
    onSelect?: (value: string) => void;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { value, disabled, onSelect, class: className, children }: Props = $props();
</script>

<CommandPrimitive.Item
  {value}
  {disabled}
  {onSelect}
  class={cn(
    'relative flex cursor-default select-none items-center rounded-sm px-2 py-1.5 text-sm outline-none data-[disabled=true]:pointer-events-none data-[selected=true]:bg-accent data-[selected=true]:text-accent-foreground data-[disabled=true]:opacity-50',
    className
  )}
>
  {#if children}
    {@render children()}
  {/if}
</CommandPrimitive.Item>
