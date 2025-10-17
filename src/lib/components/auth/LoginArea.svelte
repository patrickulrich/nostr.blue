<script lang="ts">
  // NOTE: This file is stable and usually should not be modified.
  // It is important that all functionality in this file is preserved, and should only be modified if explicitly requested.

  import { cn } from '$lib/utils';
  import AccountSwitcher from './AccountSwitcher.svelte';
  import LoginDialog from './LoginDialog.svelte';
  import SignupDialog from './SignupDialog.svelte';
  import { currentUser } from '$lib/stores/auth';

  interface Props {
    class?: string;
  }

  let { class: className }: Props = $props();

  // Get current session from Welshman
  const session = $derived(currentUser.get());

  let loginDialogOpen = $state(false);
  let signupDialogOpen = $state(false);

  function handleLogin() {
    loginDialogOpen = false;
    signupDialogOpen = false;
  }
</script>

<div class={cn("inline-flex items-center justify-center", className)}>
  {#if session}
    <AccountSwitcher onAddAccountClick={() => (loginDialogOpen = true)} />
  {:else}
    <div class="flex gap-3 justify-center">
      <!-- Log In Button -->
      <button
        type="button"
        onclick={() => (loginDialogOpen = true)}
        class="flex items-center gap-2 px-4 py-2 rounded-full bg-primary text-primary-foreground w-full font-medium transition-all hover:bg-primary/90 animate-scale-in"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M16 7a4 4 0 11-8 0 4 4 0 018 0zM12 14a7 7 0 00-7 7h14a7 7 0 00-7-7z" />
        </svg>
        <span class="truncate">Log in</span>
      </button>

      <!-- Sign Up Button -->
      <button
        type="button"
        onclick={() => (signupDialogOpen = true)}
        class="flex items-center gap-2 px-4 py-2 rounded-full font-medium transition-all border bg-background hover:bg-accent"
      >
        <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18 9v3m0 0v3m0-3h3m-3 0h-3m-2-5a4 4 0 11-8 0 4 4 0 018 0zM3 20a6 6 0 0112 0v1H3v-1z" />
        </svg>
        <span>Sign Up</span>
      </button>
    </div>
  {/if}
</div>

<LoginDialog
  bind:isOpen={loginDialogOpen}
  onClose={() => (loginDialogOpen = false)}
  onLogin={handleLogin}
  onSignup={() => (signupDialogOpen = true)}
/>

<SignupDialog
  bind:isOpen={signupDialogOpen}
  onClose={() => (signupDialogOpen = false)}
/>

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
