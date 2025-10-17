<script lang="ts">
  import { setContext } from 'svelte';
  import { cn } from '$lib/utils';

  interface Props {
    value?: string;
    maxLength: number;
    containerClass?: string;
    class?: string;
    disabled?: boolean;
    onComplete?: (value: string) => void;
    onChange?: (value: string) => void;
    children?: import('svelte').Snippet;
  }

  let {
    value = $bindable(''),
    maxLength,
    containerClass,
    class: className,
    disabled = false,
    onComplete,
    onChange,
    children
  }: Props = $props();

  let inputRef: HTMLInputElement | undefined = $state();
  let activeSlot = $state(0);

  // Split value into slots
  let slots = $derived.by(() => {
    const chars = value.split('');
    return Array.from({ length: maxLength }, (_, i) => ({
      char: chars[i] || '',
      isActive: i === activeSlot && !disabled,
      hasFakeCaret: i === activeSlot && !disabled && !chars[i]
    }));
  });

  // Update active slot based on value length
  $effect(() => {
    if (value.length < maxLength) {
      activeSlot = value.length;
    }
    if (value.length === maxLength && onComplete) {
      onComplete(value);
    }
    if (onChange) {
      onChange(value);
    }
  });

  function handleInput(e: Event) {
    const target = e.target as HTMLInputElement;
    const newValue = target.value.slice(0, maxLength);
    value = newValue;
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === 'Backspace' && value.length > 0) {
      value = value.slice(0, -1);
    }
  }

  function focusInput() {
    inputRef?.focus();
  }

  function handleContainerKeyDown(e: KeyboardEvent) {
    if (e.key === 'Enter' || e.key === ' ') {
      focusInput();
    }
  }

  setContext('inputOTP', { slots: () => slots, disabled: () => disabled });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class={cn('flex items-center gap-2', disabled && 'opacity-50', containerClass)}
  onclick={focusInput}
  onkeydown={handleContainerKeyDown}
  role="group"
  tabindex="0"
>
  <input
    bind:this={inputRef}
    type="text"
    inputmode="numeric"
    pattern="[0-9]*"
    maxlength={maxLength}
    value={value}
    oninput={handleInput}
    onkeydown={handleKeyDown}
    {disabled}
    class={cn('sr-only', className)}
    aria-label="One-time password input"
  />
  {#if children}
    {@render children()}
  {/if}
</div>
