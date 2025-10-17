<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';
  import type { ChartConfig } from './Chart.svelte';

  interface Props {
    payload?: any[];
    verticalAlign?: 'top' | 'bottom';
    hideIcon?: boolean;
    class?: string;
  }

  let {
    payload = [],
    verticalAlign = 'bottom',
    hideIcon = false,
    class: className
  }: Props = $props();

  const { config } = getContext<{ config: ChartConfig }>('chart');
</script>

{#if payload && payload.length > 0}
  <div
    class={cn(
      'flex items-center justify-center gap-4',
      verticalAlign === 'top' ? 'pb-3' : 'pt-3',
      className
    )}
  >
    {#each payload as item}
      {@const key = item.dataKey || item.value || 'value'}
      {@const itemConfig = config[key]}

      <div class="flex items-center gap-1.5">
        {#if itemConfig?.icon && !hideIcon}
          {@const Icon = itemConfig.icon}
          <Icon />
        {:else}
          <div
            class="h-2 w-2 shrink-0 rounded-[2px]"
            style="background-color: {item.color};"
          ></div>
        {/if}
        <span class="text-sm">{itemConfig?.label || item.value}</span>
      </div>
    {/each}
  </div>
{/if}
