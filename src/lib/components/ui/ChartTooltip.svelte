<script lang="ts">
  import { getContext } from 'svelte';
  import { cn } from '$lib/utils';
  import type { ChartConfig } from './Chart.svelte';

  interface Props {
    active?: boolean;
    payload?: any[];
    label?: string;
    hideLabel?: boolean;
    hideIndicator?: boolean;
    indicator?: 'line' | 'dot' | 'dashed';
    class?: string;
  }

  let {
    active = false,
    payload = [],
    label = '',
    hideLabel = false,
    hideIndicator = false,
    indicator = 'dot',
    class: className
  }: Props = $props();

  const { config } = getContext<{ config: ChartConfig }>('chart');
</script>

{#if active && payload && payload.length > 0}
  <div
    class={cn(
      'grid min-w-[8rem] items-start gap-1.5 rounded-lg border border-border/50 bg-background px-2.5 py-1.5 text-xs shadow-xl',
      className
    )}
  >
    {#if !hideLabel && label}
      <div class="font-medium">{label}</div>
    {/if}
    <div class="grid gap-1.5">
      {#each payload as item}
        {@const key = item.dataKey || item.name || 'value'}
        {@const itemConfig = config[key]}
        {@const indicatorColor = item.color || item.payload?.fill}

        <div class={cn('flex w-full items-center gap-2', indicator === 'dot' && 'items-center')}>
          {#if !hideIndicator}
            <div
              class={cn('shrink-0 rounded-[2px] border-[--color-border] bg-[--color-bg]', {
                'h-2.5 w-2.5': indicator === 'dot',
                'w-1': indicator === 'line',
                'w-0 border-[1.5px] border-dashed bg-transparent': indicator === 'dashed'
              })}
              style="--color-bg: {indicatorColor}; --color-border: {indicatorColor};"
            ></div>
          {/if}
          <div class="flex flex-1 justify-between leading-none items-center">
            <span class="text-muted-foreground">
              {itemConfig?.label || item.name}
            </span>
            {#if item.value !== undefined}
              <span class="font-mono font-medium tabular-nums text-foreground">
                {item.value.toLocaleString()}
              </span>
            {/if}
          </div>
        </div>
      {/each}
    </div>
  </div>
{/if}
