<script lang="ts">
  import { RadioGroup as RadioGroupPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    value: string;
    disabled?: boolean;
    class?: string;
  }

  let { value, disabled = false, class: className }: Props = $props();
</script>

<RadioGroupPrimitive.Item
  {value}
  {disabled}
  class={cn(
    'aspect-square h-4 w-4 rounded-full border border-primary text-primary ring-offset-background focus:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50',
    className
  )}
>
  {#snippet child({ props, checked })}
    <button {...props} type="button">
      {#if checked}
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox="0 0 24 24"
          fill="currentColor"
          class="h-2.5 w-2.5"
        >
          <circle cx="12" cy="12" r="12" />
        </svg>
      {/if}
    </button>
  {/snippet}
</RadioGroupPrimitive.Item>
