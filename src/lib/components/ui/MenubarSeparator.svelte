<script lang="ts">
  import { Menubar as MenubarPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
  }

  let { class: className }: Props = $props();
</script>

<MenubarPrimitive.Separator class={cn('-mx-1 my-1 h-px bg-muted', className)} />
