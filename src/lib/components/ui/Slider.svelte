<script lang="ts">
  import { Slider as SliderPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    value?: number[];
    min?: number;
    max?: number;
    step?: number;
    disabled?: boolean;
    onValueChange?: (value: number[]) => void;
    class?: string;
  }

  let {
    value = $bindable([0]),
    min = 0,
    max = 100,
    step = 1,
    disabled = false,
    onValueChange,
    class: className
  }: Props = $props();
</script>

<SliderPrimitive.Root
  bind:value
  {min}
  {max}
  {step}
  {disabled}
  {onValueChange}
  type="multiple"
  class={cn('relative flex w-full touch-none select-none items-center', className)}
>
  <span class="relative h-2 w-full grow overflow-hidden rounded-full bg-secondary">
    <SliderPrimitive.Range class="absolute h-full bg-primary" />
  </span>
  {#each value as _, i}
    <SliderPrimitive.Thumb
      index={i}
      class="block h-5 w-5 rounded-full border-2 border-primary bg-background ring-offset-background transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:pointer-events-none disabled:opacity-50"
    />
  {/each}
</SliderPrimitive.Root>
