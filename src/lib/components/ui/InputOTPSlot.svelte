<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';

  interface Props {
    index: number;
    class?: string;
  }

  let { index, class: className }: Props = $props();

  const context = getContext<{
    slots: () => Array<{ char: string; isActive: boolean; hasFakeCaret: boolean }>;
    disabled: () => boolean;
  }>('inputOTP');

  const slot = $derived(context?.slots()[index]);
  const disabled = $derived(context?.disabled());
</script>

<div
  class={cn(
    'relative flex h-10 w-10 items-center justify-center border-y border-r border-input text-sm transition-all first:rounded-l-md first:border-l last:rounded-r-md',
    slot?.isActive && 'z-10 ring-2 ring-ring ring-offset-background',
    disabled && 'cursor-not-allowed opacity-50',
    className
  )}
>
  {slot?.char || ''}
  {#if slot?.hasFakeCaret}
    <div class="pointer-events-none absolute inset-0 flex items-center justify-center">
      <div class="h-4 w-px animate-caret-blink bg-foreground duration-1000"></div>
    </div>
  {/if}
</div>
