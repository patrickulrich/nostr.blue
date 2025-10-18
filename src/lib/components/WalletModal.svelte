<script lang="ts">
  import { browser } from '$app/environment';
  import { cn } from '$lib/utils';
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { nwcStore } from '$lib/stores/nwc.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Textarea } from '$lib/components/ui/textarea';

  interface Props {
    children?: import('svelte').Snippet;
    class?: string;
  }

  let { children, class: className }: Props = $props();

  let isOpen = $state(false);
  let addDialogOpen = $state(false);
  let connectionUri = $state('');
  let alias = $state('');
  let isConnecting = $state(false);

  // Get reactive wallet data from stores
  const connections = $derived(nwcStore.connections);
  const activeConnection = $derived(nwcStore.activeConnection);
  const connectionInfo = $derived(nwcStore.connectionInfo);

  // WebLN detection
  const webln = $derived(
    browser && typeof globalThis !== 'undefined'
      ? (globalThis as { webln?: any }).webln
      : null
  );

  const hasNWC = $derived(
    connections.length > 0 && connections.some((c) => c.isConnected)
  );

  async function handleAddConnection() {
    if (!connectionUri.trim()) {
      alert('Connection URI required');
      return;
    }

    isConnecting = true;
    try {
      const success = await nwcStore.addConnection(connectionUri.trim(), alias.trim() || undefined);

      if (success) {
        connectionUri = '';
        alias = '';
        addDialogOpen = false;
      }
    } finally {
      isConnecting = false;
    }
  }

  function handleRemoveConnection(connectionString: string) {
    nwcStore.removeConnection(connectionString);
  }

  function handleSetActive(connectionString: string) {
    nwcStore.setActiveConnection(connectionString);
  }
</script>

<!-- Trigger Button -->
{#if children}
  <button type="button" onclick={() => (isOpen = true)} class={className}>
    {@render children()}
  </button>
{:else}
  <button
    type="button"
    onclick={() => (isOpen = true)}
    class="inline-flex items-center gap-2 px-3 py-1.5 text-sm border rounded-md hover:bg-accent {className}"
  >
    <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
    </svg>
    Wallet Settings
  </button>
{/if}

<!-- Main Wallet Dialog -->
{#if isOpen}
  <DialogPrimitive.Portal>
    <!-- Dialog Overlay -->
    <div
      class="fixed inset-0 z-50 bg-black/50"
      onclick={() => (isOpen = false)}
      role="presentation"
    >
      <!-- Dialog Content -->
      <div
        class={cn("fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 max-w-[95vw] sm:max-w-[500px] max-h-[80vh] p-6 overflow-y-auto rounded-lg bg-background shadow-lg border")}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
        role="dialog"
        tabindex="-1"
      >
        <!-- Header -->
        <div class="space-y-2 mb-6">
          <h2 class="text-lg font-semibold flex items-center gap-2">
            <svg class="h-5 w-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                stroke-width="2"
                d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z"
              />
            </svg>
            Lightning Wallet
          </h2>
          <p class="text-sm text-muted-foreground">
            Connect your lightning wallet to send zaps instantly.
          </p>
        </div>

      <div class="space-y-4">
        <!-- Wallet Status -->
        <div class="space-y-3">
          <h3 class="font-medium">Current Status</h3>
          <div class="grid gap-3">
            <!-- WebLN Status -->
            <div class="flex items-center justify-between p-3 border rounded-lg">
              <div class="flex items-center gap-3">
                <svg class="h-4 w-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M3.055 11H5a2 2 0 012 2v1a2 2 0 002 2 2 2 0 012 2v2.945M8 3.935V5.5A2.5 2.5 0 0010.5 8h.5a2 2 0 012 2 2 2 0 104 0 2 2 0 012-2h1.064M15 20.488V18a2 2 0 012-2h3.064M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                  />
                </svg>
                <div>
                  <p class="text-sm font-medium">WebLN</p>
                  <p class="text-xs text-muted-foreground">Browser extension</p>
                </div>
              </div>
              <span class="text-xs px-2 py-1 rounded {webln ? 'bg-primary text-primary-foreground' : 'bg-secondary text-secondary-foreground'}">
                {webln ? 'Ready' : 'Not Found'}
              </span>
            </div>

            <!-- NWC Status -->
            <div class="flex items-center justify-between p-3 border rounded-lg">
              <div class="flex items-center gap-3">
                <svg class="h-4 w-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    stroke-width="2"
                    d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z"
                  />
                </svg>
                <div>
                  <p class="text-sm font-medium">Nostr Wallet Connect</p>
                  <p class="text-xs text-muted-foreground">
                    {connections.length > 0
                      ? `${connections.length} wallet${connections.length !== 1 ? 's' : ''} connected`
                      : 'Remote wallet connection'}
                  </p>
                </div>
              </div>
              <span class="text-xs px-2 py-1 rounded {hasNWC ? 'bg-primary text-primary-foreground' : 'bg-secondary text-secondary-foreground'}">
                {hasNWC ? 'Ready' : 'None'}
              </span>
            </div>
          </div>
        </div>

        <div class="h-px bg-border"></div>

        <!-- NWC Management -->
        <div class="space-y-4">
          <div class="flex items-center justify-between">
            <h3 class="font-medium">Nostr Wallet Connect</h3>
            <Button
              variant="outline"
              size="sm"
              onclick={() => (addDialogOpen = true)}
            >
              <svg class="h-4 w-4 mr-1" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
              </svg>
              Add
            </Button>
          </div>

          {#if connections.length === 0}
            <div class="text-center py-6 text-muted-foreground">
              <p class="text-sm">No wallets connected</p>
            </div>
          {:else}
            <div class="space-y-2">
              {#each connections as connection}
                {@const info = connectionInfo[connection.connectionString]}
                {@const isActive = activeConnection === connection.connectionString}
                <div class="flex items-center justify-between p-3 border rounded-lg {isActive ? 'ring-2 ring-primary' : ''}">
                  <div class="flex items-center gap-3">
                    <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
                    </svg>
                    <div>
                      <p class="text-sm font-medium">
                        {connection.alias || info?.alias || 'Lightning Wallet'}
                      </p>
                      <p class="text-xs text-muted-foreground">NWC Connection</p>
                    </div>
                  </div>
                  <div class="flex items-center gap-2">
                    {#if !isActive}
                      <button
                        type="button"
                        onclick={() => handleSetActive(connection.connectionString)}
                        class="p-1 hover:bg-accent rounded"
                        aria-label="Set as active wallet"
                      >
                        <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M13 10V3L4 14h7v7l9-11h-7z" />
                        </svg>
                      </button>
                    {/if}
                    <button
                      type="button"
                      onclick={() => handleRemoveConnection(connection.connectionString)}
                      class="p-1 hover:bg-accent rounded"
                      aria-label="Remove wallet connection"
                    >
                      <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                      </svg>
                    </button>
                  </div>
                </div>
              {/each}
            </div>
          {/if}
        </div>

        {#if !webln && connections.length === 0}
          <div class="h-px bg-border"></div>
          <div class="text-center py-4">
            <p class="text-sm text-muted-foreground">
              Install a WebLN extension or connect a NWC wallet for zaps.
            </p>
          </div>
        {/if}
      </div>
    </div>
  </div>
</DialogPrimitive.Portal>
{/if}

<!-- Add Wallet Dialog -->
{#if addDialogOpen}
  <DialogPrimitive.Portal>
    <!-- Dialog Overlay -->
    <div
      class="fixed inset-0 z-50 bg-black/50"
      onclick={() => (addDialogOpen = false)}
      role="presentation"
    >
      <!-- Dialog Content -->
      <div
        class={cn("fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 max-w-[95vw] sm:max-w-[425px] p-6 overflow-y-auto rounded-lg bg-background shadow-lg border")}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
        role="dialog"
        tabindex="-1"
      >
        <!-- Header -->
        <div class="space-y-2 mb-6">
          <h2 class="text-lg font-semibold">Connect NWC Wallet</h2>
          <p class="text-sm text-muted-foreground">
            Enter your connection string from a compatible wallet.
          </p>
        </div>

      <div class="space-y-4">
        <div class="space-y-2">
          <Label for="alias">Wallet Name (optional)</Label>
          <Input
            id="alias"
            type="text"
            placeholder="My Lightning Wallet"
            bind:value={alias}
          />
        </div>

        <div class="space-y-2">
          <Label for="connection-uri">Connection URI</Label>
          <Textarea
            id="connection-uri"
            placeholder="nostr+walletconnect://..."
            bind:value={connectionUri}
            rows={3}
            class="resize-none"
          />
        </div>
      </div>

        <!-- Footer -->
        <div class="flex justify-end gap-2 mt-6">
          <Button
            variant="outline"
            onclick={() => (addDialogOpen = false)}
          >
            Cancel
          </Button>
          <Button
            onclick={handleAddConnection}
            disabled={isConnecting || !connectionUri.trim()}
          >
            {isConnecting ? 'Connecting...' : 'Connect'}
          </Button>
        </div>
      </div>
    </div>
  </DialogPrimitive.Portal>
{/if}
