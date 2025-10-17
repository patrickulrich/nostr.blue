<script lang="ts">
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
  }

  let { class: className }: Props = $props();
</script>

<div class={cn('-mx-1 my-1 h-px bg-muted', className)}></div>
