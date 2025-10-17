<script lang="ts">
  import { Switch as SwitchPrimitive } from 'bits-ui';
  import { cn } from '$lib/utils';

  interface Props {
    checked?: boolean;
    onCheckedChange?: (checked: boolean) => void;
    disabled?: boolean;
    name?: string;
    value?: string;
    required?: boolean;
    class?: string;
  }

  let {
    checked = $bindable(false),
    onCheckedChange,
    disabled = false,
    name,
    value,
    required,
    class: className
  }: Props = $props();
</script>

<SwitchPrimitive.Root
  bind:checked
  {onCheckedChange}
  {disabled}
  {name}
  {value}
  {required}
  class={cn(
    'peer inline-flex h-6 w-11 shrink-0 cursor-pointer items-center rounded-full border-2 border-transparent transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-primary data-[state=unchecked]:bg-input',
    className
  )}
>
  <SwitchPrimitive.Thumb
    class="pointer-events-none block h-5 w-5 rounded-full bg-background shadow-lg ring-0 transition-transform data-[state=checked]:translate-x-5 data-[state=unchecked]:translate-x-0"
  />
</SwitchPrimitive.Root>
