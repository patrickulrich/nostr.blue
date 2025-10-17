<script lang="ts">
  import { Progress as ProgressPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    value?: number | null;
    max?: number;
    class?: string;
  }

  let { value = 0, max = 100, class: className }: Props = $props();
</script>

<ProgressPrimitive.Root
  {value}
  {max}
  class={cn('relative h-4 w-full overflow-hidden rounded-full bg-secondary', className)}
>
  {#snippet child({ props })}
    <div
      {...props}
      class="h-full w-full flex-1 bg-primary transition-all"
      style="transform: translateX(-{100 - ((value ?? 0) / max) * 100}%)"
    ></div>
  {/snippet}
</ProgressPrimitive.Root>
