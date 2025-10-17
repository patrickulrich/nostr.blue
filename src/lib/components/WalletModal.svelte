<script lang="ts">
  import { browser } from '$app/environment';

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

  // TODO: Integrate with NWC store/hooks
  // import { nwcStore } from '$lib/stores/nwc';
  // const connections = $derived($nwcStore.connections);
  // const activeConnection = $derived($nwcStore.activeConnection);
  // const connectionInfo = $derived($nwcStore.connectionInfo);

  // Placeholder data
  const connections: any[] = [];
  const activeConnection: string | null = null;
  const connectionInfo: Record<string, any> = {};

  // TODO: Integrate with WebLN detection
  // import { webln } from '$lib/stores/wallet';
  const webln = $state<any>(null);

  const hasNWC = $derived(
    connections.length > 0 && connections.some((c: any) => c.isConnected)
  );

  // TODO: Integrate with mobile detection
  // import { isMobile } from '$lib/stores/device';
  const isMobile = $state(false);

  async function handleAddConnection() {
    if (!connectionUri.trim()) {
      alert('Connection URI required');
      return;
    }

    isConnecting = true;
    try {
      // TODO: Implement NWC connection
      // await nwcStore.addConnection(connectionUri.trim(), alias.trim() || undefined);
      console.log('Add NWC connection:', connectionUri, alias);

      connectionUri = '';
      alias = '';
      addDialogOpen = false;
    } finally {
      isConnecting = false;
    }
  }

  function handleRemoveConnection(connectionString: string) {
    // TODO: Implement connection removal
    // nwcStore.removeConnection(connectionString);
    console.log('Remove connection:', connectionString);
  }

  function handleSetActive(connectionString: string) {
    // TODO: Implement active connection setting
    // nwcStore.setActiveConnection(connectionString);
    console.log('Set active connection:', connectionString);
    alert('Active wallet changed');
  }
</script>

<!-- Main Modal/Drawer -->
{#if !isMobile}
  <!-- Desktop Dialog -->
  {#if isOpen}
    <div
      class="fixed inset-0 z-50 bg-black/50"
      onclick={() => (isOpen = false)}
      role="presentation"
    >
      <div
        class="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-[500px] max-h-[80vh] overflow-y-auto bg-background rounded-lg shadow-lg"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
        role="dialog"
        tabindex="-1"
      >
        <div class="p-6 space-y-4">
          <div class="space-y-2">
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
              <button
                type="button"
                onclick={() => (addDialogOpen = true)}
                class="px-3 py-1.5 text-sm border rounded-md hover:bg-accent"
              >
                <span class="inline-flex items-center gap-1">
                  <svg class="h-4 w-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                  </svg>
                  Add
                </span>
              </button>
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
  {/if}

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

  <!-- Add Wallet Dialog -->
  {#if addDialogOpen}
    <div
      class="fixed inset-0 z-50 bg-black/50"
      onclick={() => (addDialogOpen = false)}
      role="presentation"
    >
      <div
        class="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 w-full max-w-[425px] bg-background rounded-lg shadow-lg p-6"
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
        role="dialog"
        tabindex="-1"
      >
        <h2 class="text-lg font-semibold mb-2">Connect NWC Wallet</h2>
        <p class="text-sm text-muted-foreground mb-4">
          Enter your connection string from a compatible wallet.
        </p>

        <div class="space-y-4">
          <div>
            <label for="alias" class="block text-sm font-medium mb-1">
              Wallet Name (optional)
            </label>
            <input
              id="alias"
              type="text"
              placeholder="My Lightning Wallet"
              bind:value={alias}
              class="w-full px-3 py-2 border rounded-md bg-background"
            />
          </div>

          <div>
            <label for="connection-uri" class="block text-sm font-medium mb-1">
              Connection URI
            </label>
            <textarea
              id="connection-uri"
              placeholder="nostr+walletconnect://..."
              bind:value={connectionUri}
              rows="3"
              class="w-full px-3 py-2 border rounded-md bg-background resize-none"
            ></textarea>
          </div>

          <button
            type="button"
            onclick={handleAddConnection}
            disabled={isConnecting || !connectionUri.trim()}
            class="w-full px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {isConnecting ? 'Connecting...' : 'Connect'}
          </button>
        </div>
      </div>
    </div>
  {/if}
{:else}
  <!-- TODO: Mobile Drawer implementation -->
  <div class="text-muted-foreground text-sm p-4">
    Mobile wallet modal not yet implemented
  </div>
{/if}
