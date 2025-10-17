<script lang="ts">
  import { cn } from '$lib/utils';
  import Separator from './Separator.svelte';

  interface Props {
    class?: string;
  }

  let { class: className }: Props = $props();
</script>

<Separator class={cn('mx-2 w-auto bg-sidebar-border', className)} />
