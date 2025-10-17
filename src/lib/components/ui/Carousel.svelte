<script lang="ts">
  import { setContext } from 'svelte';
  import emblaCarouselSvelte from 'embla-carousel-svelte';
  import type { EmblaOptionsType, EmblaCarouselType } from 'embla-carousel';
  import { cn } from '$lib/utils';

  interface Props {
    opts?: EmblaOptionsType;
    orientation?: 'horizontal' | 'vertical';
    class?: string;
    children?: import('svelte').Snippet;
  }

  let {
    opts,
    orientation = 'horizontal',
    class: className,
    children
  }: Props = $props();

  let emblaNode: HTMLDivElement;
  let emblaApi: EmblaCarouselType | undefined = $state(undefined);
  let canScrollPrev = $state(false);
  let canScrollNext = $state(false);

  const finalOpts: EmblaOptionsType = {
    ...opts,
    axis: orientation === 'horizontal' ? 'x' : 'y'
  };

  function scrollPrev() {
    emblaApi?.scrollPrev();
  }

  function scrollNext() {
    emblaApi?.scrollNext();
  }

  function onSelect(api: EmblaCarouselType) {
    canScrollPrev = api.canScrollPrev();
    canScrollNext = api.canScrollNext();
  }

  function onInit(event: CustomEvent<EmblaCarouselType>) {
    emblaApi = event.detail;
    onSelect(emblaApi);
    emblaApi.on('select', () => onSelect(emblaApi!));
    emblaApi.on('reInit', () => onSelect(emblaApi!));
  }

  function handleKeyDown(event: KeyboardEvent) {
    if (event.key === 'ArrowLeft') {
      event.preventDefault();
      scrollPrev();
    } else if (event.key === 'ArrowRight') {
      event.preventDefault();
      scrollNext();
    }
  }

  setContext('carousel', {
    orientation: orientation || (opts?.axis === 'y' ? 'vertical' : 'horizontal'),
    scrollPrev,
    scrollNext,
    canScrollPrev: () => canScrollPrev,
    canScrollNext: () => canScrollNext
  });
</script>

<div
  bind:this={emblaNode}
  use:emblaCarouselSvelte={finalOpts}
  onemblaInit={onInit}
  class={cn('relative', className)}
  role="region"
  aria-roledescription="carousel"
  onkeydown={handleKeyDown}
  tabindex="0"
>
  {#if children}
    {@render children()}
  {/if}
</div>
