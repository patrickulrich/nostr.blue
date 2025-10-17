<script lang="ts">
  import { appConfig, presetRelays } from '$lib/stores/appStore';
  import { cn } from '$lib/utils';

  interface Props {
    class?: string;
  }

  let { class: className }: Props = $props();

  let isOpen = $state(false);
  let inputValue = $state('');

  const selectedRelay = $derived($appConfig.relayUrl);

  const selectedOption = $derived(
    presetRelays.find((option) => option.url === selectedRelay)
  );

  // Function to normalize relay URL by adding wss:// if no protocol is present
  function normalizeRelayUrl(url: string): string {
    const trimmed = url.trim();
    if (!trimmed) return trimmed;

    // Check if it already has a protocol
    if (trimmed.includes('://')) {
      return trimmed;
    }

    // Add wss:// prefix
    return `wss://${trimmed}`;
  }

  // Handle selecting a relay
  function selectRelay(url: string) {
    appConfig.updateRelayUrl(normalizeRelayUrl(url));
    isOpen = false;
    inputValue = '';
  }

  // Handle adding a custom relay
  function handleAddCustomRelay(url: string) {
    selectRelay(normalizeRelayUrl(url));
  }

  // Check if input value looks like a valid relay URL
  function isValidRelayInput(value: string): boolean {
    const trimmed = value.trim();
    if (!trimmed) return false;

    // Basic validation - should contain at least a domain-like structure
    const normalized = normalizeRelayUrl(trimmed);
    try {
      new URL(normalized);
      return true;
    } catch {
      return false;
    }
  }

  // Filter preset relays based on input
  const filteredRelays = $derived(
    presetRelays.filter(
      (option) =>
        !inputValue ||
        option.name.toLowerCase().includes(inputValue.toLowerCase()) ||
        option.url.toLowerCase().includes(inputValue.toLowerCase())
    )
  );

  // Close dropdown when clicking outside
  function handleClickOutside(event: MouseEvent) {
    const target = event.target as HTMLElement;
    if (!target.closest('.relay-selector')) {
      isOpen = false;
    }
  }

  $effect(() => {
    if (isOpen) {
      document.addEventListener('click', handleClickOutside);
      return () => document.removeEventListener('click', handleClickOutside);
    }
  });
</script>

<div class={cn("relay-selector relative", className)}>
  <!-- Trigger Button -->
  <button
    type="button"
    onclick={() => (isOpen = !isOpen)}
    class="flex items-center justify-between w-full px-3 py-2 text-sm border rounded-md bg-background hover:bg-accent hover:text-accent-foreground"
  >
    <div class="flex items-center gap-2">
      <!-- Wifi Icon (simplified) -->
      <svg
        class="h-4 w-4"
        fill="none"
        stroke="currentColor"
        viewBox="0 0 24 24"
        xmlns="http://www.w3.org/2000/svg"
      >
        <path
          stroke-linecap="round"
          stroke-linejoin="round"
          stroke-width="2"
          d="M8.111 16.404a5.5 5.5 0 017.778 0M12 20h.01m-7.08-7.071c3.904-3.905 10.236-3.905 14.141 0M1.394 9.393c5.857-5.857 15.355-5.857 21.213 0"
        />
      </svg>
      <span class="truncate">
        {selectedOption
          ? selectedOption.name
          : selectedRelay
            ? selectedRelay.replace(/^wss?:\/\//, '')
            : 'Select relay...'}
      </span>
    </div>
    <!-- Chevron Icon -->
    <svg
      class="h-4 w-4 opacity-50"
      fill="none"
      stroke="currentColor"
      viewBox="0 0 24 24"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path
        stroke-linecap="round"
        stroke-linejoin="round"
        stroke-width="2"
        d="M8 9l4-4 4 4m0 6l-4 4-4-4"
      />
    </svg>
  </button>

  <!-- Dropdown Content -->
  {#if isOpen}
    <div
      class="absolute z-50 mt-2 w-full min-w-[300px] rounded-md border bg-popover p-0 shadow-md"
    >
      <!-- Search Input -->
      <div class="p-2">
        <input
          type="text"
          placeholder="Search relays or type URL..."
          bind:value={inputValue}
          class="w-full px-3 py-2 text-sm border rounded-md bg-background"
          onclick={(e) => e.stopPropagation()}
        />
      </div>

      <!-- Relay List -->
      <div class="max-h-[300px] overflow-y-auto p-1">
        {#if filteredRelays.length === 0 && !isValidRelayInput(inputValue)}
          <div class="py-6 text-center text-sm text-muted-foreground">
            {inputValue ? 'Invalid relay URL' : 'No relay found.'}
          </div>
        {/if}

        {#each filteredRelays as option}
          <button
            type="button"
            onclick={() => selectRelay(option.url)}
            class="flex w-full items-center gap-2 px-2 py-1.5 text-sm rounded-sm hover:bg-accent cursor-pointer"
          >
            <!-- Check Icon -->
            <svg
              class={cn(
                'h-4 w-4',
                selectedRelay === option.url ? 'opacity-100' : 'opacity-0'
              )}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M5 13l4 4L19 7"
              />
            </svg>
            <div class="flex flex-col text-left">
              <span class="font-medium">{option.name}</span>
              <span class="text-xs text-muted-foreground">{option.url}</span>
            </div>
          </button>
        {/each}

        <!-- Add Custom Relay Option -->
        {#if inputValue && isValidRelayInput(inputValue)}
          <button
            type="button"
            onclick={() => handleAddCustomRelay(inputValue)}
            class="flex w-full items-center gap-2 px-2 py-1.5 text-sm rounded-sm hover:bg-accent cursor-pointer border-t mt-1 pt-2"
          >
            <!-- Plus Icon -->
            <svg
              class="h-4 w-4"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
              xmlns="http://www.w3.org/2000/svg"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M12 4v16m8-8H4"
              />
            </svg>
            <div class="flex flex-col text-left">
              <span class="font-medium">Add custom relay</span>
              <span class="text-xs text-muted-foreground">
                {normalizeRelayUrl(inputValue)}
              </span>
            </div>
          </button>
        {/if}
      </div>
    </div>
  {/if}
</div>
