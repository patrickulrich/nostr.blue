<script lang="ts">
  // NOTE: This file is stable and usually should not be modified.
  // It is important that all functionality in this file is preserved, and should only be modified if explicitly requested.

  import { cn } from '$lib/utils';
  import { browser } from '$app/environment';
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import {
    loginWithExtension,
    loginWithNsec,
    loginWithBunker,
    hasNostrExtension,
    validateNsec,
    validateBunkerUri
  } from '$lib/stores/auth';

  interface Props {
    isOpen: boolean;
    onClose: () => void;
    onLogin: () => void;
    onSignup?: () => void;
  }

  let { isOpen = $bindable(), onClose, onLogin, onSignup }: Props = $props();

  let isLoading = $state(false);
  let isFileLoading = $state(false);
  let nsec = $state('');
  let bunkerUri = $state('');
  let errors = $state<{
    nsec?: string;
    bunker?: string;
    file?: string;
    extension?: string;
  }>({});
  let fileInputRef = $state<HTMLInputElement | undefined>(undefined);
  let currentTab = $state<'extension' | 'key' | 'bunker'>('extension');

  // Reset state when dialog opens/closes
  $effect(() => {
    if (isOpen) {
      isLoading = false;
      isFileLoading = false;
      nsec = '';
      bunkerUri = '';
      errors = {};
      if (fileInputRef) {
        fileInputRef.value = '';
      }
      // Set default tab based on extension availability
      if (browser && hasNostrExtension()) {
        currentTab = 'extension';
      } else {
        currentTab = 'key';
      }
    }
  });

  async function handleExtensionLogin() {
    isLoading = true;
    errors = { ...errors, extension: undefined };

    try {
      await loginWithExtension();
      onLogin();
      onClose();
    } catch (e: unknown) {
      const error = e as Error;
      console.error('Extension login failed:', error);
      errors = {
        ...errors,
        extension: error instanceof Error ? error.message : 'Extension login failed'
      };
    } finally {
      isLoading = false;
    }
  }

  function executeLogin(key: string) {
    isLoading = true;
    errors = {};

    // Use a timeout to allow the UI to update before the synchronous login call
    setTimeout(() => {
      try {
        loginWithNsec(key);
        onLogin();
        onClose();
      } catch (e: unknown) {
        const error = e as Error;
        errors = { nsec: error instanceof Error ? error.message : "Failed to login with this key. Please check that it's correct." };
        isLoading = false;
      }
    }, 50);
  }

  function handleKeyLogin() {
    if (!nsec.trim()) {
      errors = { ...errors, nsec: 'Please enter your secret key' };
      return;
    }

    if (!validateNsec(nsec)) {
      errors = { ...errors, nsec: 'Invalid secret key format. Must be a valid nsec starting with nsec1.' };
      return;
    }
    executeLogin(nsec);
  }

  async function handleBunkerLogin() {
    if (!bunkerUri.trim()) {
      errors = { ...errors, bunker: 'Please enter a bunker URI' };
      return;
    }

    if (!validateBunkerUri(bunkerUri)) {
      errors = { ...errors, bunker: 'Invalid bunker URI format. Must start with bunker://' };
      return;
    }

    isLoading = true;
    errors = { ...errors, bunker: undefined };

    try {
      await loginWithBunker(bunkerUri);
      onLogin();
      onClose();
      bunkerUri = '';
    } catch (e: unknown) {
      const error = e as Error;
      errors = {
        ...errors,
        bunker: error instanceof Error ? error.message : 'Failed to connect to bunker. Please check the URI.'
      };
    } finally {
      isLoading = false;
    }
  }

  function handleFileUpload(e: Event) {
    const target = e.target as HTMLInputElement;
    const file = target.files?.[0];
    if (!file) return;

    isFileLoading = true;
    errors = {};

    const reader = new FileReader();
    reader.onload = (event) => {
      isFileLoading = false;
      const content = event.target?.result as string;
      if (content) {
        const trimmedContent = content.trim();
        if (validateNsec(trimmedContent)) {
          executeLogin(trimmedContent);
        } else {
          errors = { file: 'File does not contain a valid secret key.' };
        }
      } else {
        errors = { file: 'Could not read file content.' };
      }
    };
    reader.onerror = () => {
      isFileLoading = false;
      errors = { file: 'Failed to read file.' };
    };
    reader.readAsText(file);
  }

  function handleSignupClick() {
    onClose();
    if (onSignup) {
      onSignup();
    }
  }
</script>

{#if isOpen}
  <DialogPrimitive.Portal>
    <!-- Dialog Overlay -->
    <div
      class="fixed inset-0 z-50 bg-black/50"
      onclick={onClose}
      role="presentation"
    >
      <!-- Dialog Content -->
      <div
        class={cn("fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 max-w-[95vw] sm:max-w-md max-h-[90vh] max-h-[90dvh] p-0 overflow-hidden rounded-2xl overflow-y-scroll bg-background shadow-lg")}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
        role="dialog"
        tabindex="-1"
      >
      <!-- Header -->
      <div class="px-6 pt-6 pb-1 relative">
        <p class="text-center text-muted-foreground text-sm">
          Sign up or log in to continue
        </p>
      </div>

      <!-- Content -->
      <div class="px-6 pt-2 pb-4 space-y-4 overflow-y-auto flex-1">
        <!-- Prominent Sign Up Section -->
        <div class="relative p-4 rounded-2xl bg-gradient-to-br from-blue-50 to-indigo-100 dark:from-blue-950/50 dark:to-indigo-950/50 border border-blue-200 dark:border-blue-800 overflow-hidden">
          <div class="relative z-10 text-center space-y-3">
            <div class="flex justify-center items-center gap-2 mb-2">
              <svg class="w-5 h-5 text-blue-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 3v4M3 5h4M6 17v4m-2-2h4m5-16l2.286 6.857L21 12l-5.714 2.143L13 21l-2.286-6.857L5 12l5.714-2.143L13 3z" />
              </svg>
              <span class="font-semibold text-blue-800 dark:text-blue-200">
                New to Nostr?
              </span>
            </div>
            <p class="text-sm text-blue-700 dark:text-blue-300">
              Create a new account to get started. It's free and open.
            </p>
            <button
              type="button"
              onclick={handleSignupClick}
              class="w-full rounded-full py-3 text-base font-semibold bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-700 hover:to-indigo-700 transform transition-all duration-200 hover:scale-105 shadow-lg border-0 text-white"
            >
              <svg class="w-4 h-4 inline-block mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18 9v3m0 0v3m0-3h3m-3 0h-3m-2-5a4 4 0 11-8 0 4 4 0 018 0zM3 20a6 6 0 0112 0v1H3v-1z" />
              </svg>
              <span>Sign Up</span>
            </button>
          </div>
        </div>

        <!-- Divider -->
        <div class="relative">
          <div class="absolute inset-0 flex items-center">
            <div class="w-full border-t border-border"></div>
          </div>
          <div class="relative flex justify-center text-sm">
            <span class="px-3 bg-background text-muted-foreground">Or log in</span>
          </div>
        </div>

        <!-- Login Tabs -->
        <div class="w-full">
          <!-- Tab List -->
          <div class="grid w-full grid-cols-3 bg-muted/80 rounded-lg mb-4 p-1">
            <button
              type="button"
              onclick={() => (currentTab = 'extension')}
              class="flex items-center justify-center gap-2 px-3 py-2 rounded-md text-sm transition-colors {currentTab === 'extension' ? 'bg-background shadow-sm' : 'hover:bg-background/50'}"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
              </svg>
              <span>Extension</span>
            </button>
            <button
              type="button"
              onclick={() => (currentTab = 'key')}
              class="flex items-center justify-center gap-2 px-3 py-2 rounded-md text-sm transition-colors {currentTab === 'key' ? 'bg-background shadow-sm' : 'hover:bg-background/50'}"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
              </svg>
              <span>Key</span>
            </button>
            <button
              type="button"
              onclick={() => (currentTab = 'bunker')}
              class="flex items-center justify-center gap-2 px-3 py-2 rounded-md text-sm transition-colors {currentTab === 'bunker' ? 'bg-background shadow-sm' : 'hover:bg-background/50'}"
            >
              <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 15a4 4 0 004 4h9a5 5 0 10-.1-9.999 5.002 5.002 0 10-9.78 2.096A4.001 4.001 0 003 15z" />
              </svg>
              <span>Bunker</span>
            </button>
          </div>

          <!-- Tab Content: Extension -->
          {#if currentTab === 'extension'}
            <div class="space-y-3">
              {#if errors.extension}
                <div class="p-3 rounded-md bg-destructive/10 text-destructive text-sm flex items-start gap-2">
                  <svg class="w-4 h-4 mt-0.5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
                  </svg>
                  <span>{errors.extension}</span>
                </div>
              {/if}
              <div class="text-center p-4 rounded-lg bg-muted">
                <svg class="w-12 h-12 mx-auto mb-3 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z" />
                </svg>
                <p class="text-sm text-muted-foreground mb-4">
                  Login with one click using the browser extension
                </p>
                <button
                  type="button"
                  class="w-full rounded-full py-4 bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                  onclick={handleExtensionLogin}
                  disabled={isLoading}
                >
                  {isLoading ? 'Logging in...' : 'Login with Extension'}
                </button>
              </div>
            </div>
          {/if}

          <!-- Tab Content: Key -->
          {#if currentTab === 'key'}
            <div class="space-y-4">
              <div class="space-y-2">
                <label for="nsec" class="text-sm font-medium">
                  Secret Key (nsec)
                </label>
                <input
                  id="nsec"
                  type="password"
                  bind:value={nsec}
                  oninput={() => { if (errors.nsec) errors = { ...errors, nsec: undefined }; }}
                  class="w-full px-3 py-2 border rounded-lg bg-background {errors.nsec ? 'border-destructive' : ''}"
                  placeholder="nsec1..."
                  autocomplete="off"
                />
                {#if errors.nsec}
                  <p class="text-sm text-destructive">{errors.nsec}</p>
                {/if}
              </div>

              <button
                type="button"
                class="w-full rounded-full py-3 bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                onclick={handleKeyLogin}
                disabled={isLoading || !nsec.trim()}
              >
                {isLoading ? 'Verifying...' : 'Log In'}
              </button>

              <div class="relative">
                <div class="absolute inset-0 flex items-center">
                  <div class="w-full border-t border-border"></div>
                </div>
                <div class="relative flex justify-center text-xs">
                  <span class="px-2 bg-background text-muted-foreground">or</span>
                </div>
              </div>

              <div class="text-center">
                <input
                  type="file"
                  accept=".txt"
                  class="hidden"
                  bind:this={fileInputRef}
                  onchange={handleFileUpload}
                />
                <button
                  type="button"
                  class="w-full px-4 py-2 border rounded-md hover:bg-accent disabled:opacity-50"
                  onclick={() => fileInputRef?.click()}
                  disabled={isLoading || isFileLoading}
                >
                  <svg class="w-4 h-4 inline-block mr-2" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 6L16 6a5 5 0 011 9.9M15 13l-3-3m0 0l-3 3m3-3v12" />
                  </svg>
                  {isFileLoading ? 'Reading File...' : 'Upload Your Key File'}
                </button>
                {#if errors.file}
                  <p class="text-sm text-destructive mt-2">{errors.file}</p>
                {/if}
              </div>
            </div>
          {/if}

          <!-- Tab Content: Bunker -->
          {#if currentTab === 'bunker'}
            <div class="space-y-3">
              <div class="space-y-2">
                <label for="bunkerUri" class="text-sm font-medium">
                  Bunker URI
                </label>
                <input
                  id="bunkerUri"
                  type="text"
                  bind:value={bunkerUri}
                  oninput={() => { if (errors.bunker) errors = { ...errors, bunker: undefined }; }}
                  class="w-full px-3 py-2 border rounded-lg bg-background {errors.bunker ? 'border-destructive' : ''}"
                  placeholder="bunker://"
                  autocomplete="off"
                />
                {#if errors.bunker}
                  <p class="text-sm text-destructive">{errors.bunker}</p>
                {/if}
              </div>

              <button
                type="button"
                class="w-full rounded-full py-4 bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50"
                onclick={handleBunkerLogin}
                disabled={isLoading || !bunkerUri.trim()}
              >
                {isLoading ? 'Connecting...' : 'Login with Bunker'}
              </button>
            </div>
          {/if}
        </div>
      </div>
    </div>
    </div>
  </DialogPrimitive.Portal>
{/if}
