<script lang="ts">
  // NOTE: This file is stable and usually should not be modified.
  // It is important that all functionality in this file is preserved, and should only be modified if explicitly requested.

  import WalletModal from '$lib/components/WalletModal.svelte';
  import { genUserName } from '$lib/genUserName';
  import { currentUser, otherSessions, switchAccount, logout } from '$lib/stores/auth';
  import { profilesByPubkey } from '@welshman/app';
  import { derived } from 'svelte/store';

  interface Props {
    onAddAccountClick: () => void;
  }

  let { onAddAccountClick }: Props = $props();

  let isDropdownOpen = $state(false);

  // Get current session and other sessions from Welshman
  const otherUsers = $derived($otherSessions);

  // Create a reactive store for current user's profile
  const currentUserProfile = derived(
    [currentUser, profilesByPubkey],
    ([$currentUser, $profilesByPubkey]) => {
      if (!$currentUser?.pubkey) return null;
      return $profilesByPubkey.get($currentUser.pubkey);
    }
  );

  function getDisplayName(account: any): string {
    if (!account?.pubkey) return 'Unknown';

    // Check if this is the current user and we have profile data
    if ($currentUser && account.pubkey === $currentUser.pubkey) {
      return $currentUserProfile?.name || genUserName(account.pubkey);
    }

    return genUserName(account.pubkey);
  }

  function getProfilePicture(account: any): string | undefined {
    if (!account?.pubkey) return undefined;

    // Check if this is the current user and we have profile data
    if ($currentUser && account.pubkey === $currentUser.pubkey) {
      return $currentUserProfile?.picture;
    }

    return undefined;
  }

  function setLogin(accountPubkey: string) {
    try {
      switchAccount(accountPubkey);
      isDropdownOpen = false;
    } catch (e: unknown) {
      const error = e as Error;
      console.error('Failed to switch account:', error);
      alert(error instanceof Error ? error.message : 'Failed to switch account');
    }
  }

  function removeLogin(accountPubkey: string) {
    try {
      logout(accountPubkey);
      isDropdownOpen = false;
    } catch (e: unknown) {
      const error = e as Error;
      console.error('Failed to logout:', error);
      alert(error instanceof Error ? error.message : 'Failed to logout');
    }
  }

  // Close dropdown when clicking outside
  function handleClickOutside(event: MouseEvent) {
    const target = event.target as HTMLElement;
    if (!target.closest('.account-switcher-dropdown')) {
      isDropdownOpen = false;
    }
  }

  $effect(() => {
    if (isDropdownOpen) {
      // Add listener on next tick to avoid catching the opening click
      setTimeout(() => {
        document.addEventListener('click', handleClickOutside);
      }, 0);
      return () => document.removeEventListener('click', handleClickOutside);
    }
  });
</script>

{#if $currentUser}
  <div class="account-switcher-dropdown relative">
    <!-- Trigger Button -->
    <button
      type="button"
      onclick={(e) => {
        e.stopPropagation();
        isDropdownOpen = !isDropdownOpen;
      }}
      class="flex items-center gap-3 p-3 rounded-full hover:bg-accent transition-all w-full text-foreground"
    >
      <!-- Avatar -->
      <div class="w-10 h-10 rounded-full overflow-hidden bg-muted flex items-center justify-center">
        {#if getProfilePicture($currentUser)}
          <img src={getProfilePicture($currentUser)} alt={getDisplayName($currentUser)} class="w-full h-full object-cover" />
        {:else}
          <span class="text-lg font-medium">{getDisplayName($currentUser).charAt(0)}</span>
        {/if}
      </div>

      <div class="flex-1 text-left hidden md:block truncate">
        <p class="font-medium text-sm truncate">{getDisplayName($currentUser)}</p>
      </div>

      <!-- Chevron Down Icon -->
      <svg class="w-4 h-4 text-muted-foreground" fill="none" stroke="currentColor" viewBox="0 0 24 24">
        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M19 9l-7 7-7-7" />
      </svg>
    </button>

    <!-- Dropdown Content -->
    {#if isDropdownOpen}
      <div class="absolute bottom-full right-0 mb-2 w-64 bg-popover border rounded-md shadow-lg p-2 z-50 animate-scale-in">
        <!-- Switch Account Section -->
        <div class="font-medium text-sm px-2 py-1.5">Switch Account</div>
        {#each otherUsers as user}
          <button
            type="button"
            onclick={() => setLogin(user.pubkey)}
            class="flex items-center gap-2 w-full cursor-pointer p-2 rounded-md hover:bg-accent text-left"
          >
            <div class="w-8 h-8 rounded-full overflow-hidden bg-muted flex items-center justify-center shrink-0">
              {#if user.metadata?.picture}
                <img src={user.metadata.picture} alt={getDisplayName(user)} class="w-full h-full object-cover" />
              {:else}
                <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
                </svg>
              {/if}
            </div>
            <div class="flex-1 truncate">
              <p class="text-sm font-medium">{getDisplayName(user)}</p>
            </div>
            {#if user.pubkey === $currentUser.pubkey}
              <div class="w-2 h-2 rounded-full bg-primary"></div>
            {/if}
          </button>
        {/each}

        <div class="h-px bg-border my-2"></div>

        <!-- Wallet Settings -->
        <WalletModal>
          <div class="flex items-center gap-2 w-full cursor-pointer p-2 rounded-md hover:bg-accent text-left">
            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M3 10h18M7 15h1m4 0h1m-7 4h12a3 3 0 003-3V8a3 3 0 00-3-3H6a3 3 0 00-3 3v8a3 3 0 003 3z" />
            </svg>
            <span>Wallet Settings</span>
          </div>
        </WalletModal>

        <!-- Add Another Account -->
        <button
          type="button"
          onclick={onAddAccountClick}
          class="flex items-center gap-2 w-full cursor-pointer p-2 rounded-md hover:bg-accent text-left"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18 9v3m0 0v3m0-3h3m-3 0h-3m-2-5a4 4 0 11-8 0 4 4 0 018 0zM3 20a6 6 0 0112 0v1H3v-1z" />
          </svg>
          <span>Add another account</span>
        </button>

        <!-- Log Out -->
        <button
          type="button"
          onclick={() => removeLogin($currentUser.pubkey)}
          class="flex items-center gap-2 w-full cursor-pointer p-2 rounded-md hover:bg-accent text-left text-red-500"
        >
          <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
          </svg>
          <span>Log out</span>
        </button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .animate-scale-in {
    animation: scale-in 0.2s ease-out;
  }

  @keyframes scale-in {
    from {
      opacity: 0;
      transform: scale(0.95);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }
</style>
