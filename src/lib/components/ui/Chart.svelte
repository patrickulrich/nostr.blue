<script lang="ts">
  import { setContext } from 'svelte';
  import { cn } from '$lib/utils';

  export interface ChartConfig {
    [k: string]: {
      label?: string;
      icon?: any;
    } & (
      | { color?: string; theme?: never }
      | { color?: never; theme: { light: string; dark: string } }
    );
  }

  interface Props {
    config: ChartConfig;
    id?: string;
    class?: string;
    children?: import('svelte').Snippet;
  }

  let { config, id, class: className, children }: Props = $props();

  const chartId = id || `chart-${Math.random().toString(36).substr(2, 9)}`;

  // Build CSS custom properties for chart colors
  let chartStyles = $derived.by(() => {
    const colorConfig = Object.entries(config).filter(
      ([_, cfg]) => cfg.theme || cfg.color
    );

    if (colorConfig.length === 0) return '';

    const lightVars = colorConfig
      .map(([key, cfg]) => {
        const color = cfg.theme?.light || cfg.color;
        return color ? `  --color-${key}: ${color};` : '';
      })
      .filter(Boolean)
      .join('\n');

    const darkVars = colorConfig
      .map(([key, cfg]) => {
        const color = cfg.theme?.dark || cfg.color;
        return color ? `  --color-${key}: ${color};` : '';
      })
      .filter(Boolean)
      .join('\n');

    return `
      [data-chart="${chartId}"] {
${lightVars}
      }

      .dark [data-chart="${chartId}"] {
${darkVars}
      }
    `;
  });

  setContext('chart', { config });

  // Apply dynamic styles via CSS custom properties directly on the element
  let dynamicStyles = $derived.by(() => {
    const colorConfig = Object.entries(config).filter(
      ([_, cfg]) => cfg.theme || cfg.color
    );

    if (colorConfig.length === 0) return '';

    return colorConfig
      .map(([key, cfg]) => {
        const color = cfg.theme?.light || cfg.color;
        return color ? `--color-${key}: ${color};` : '';
      })
      .filter(Boolean)
      .join(' ');
  });
</script>

<div
  data-chart={chartId}
  class={cn(
    'flex aspect-video justify-center text-xs',
    className
  )}
  style={dynamicStyles}
>
  {#if children}
    {@render children()}
  {/if}
</div>
