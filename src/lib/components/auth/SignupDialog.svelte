<script lang="ts">
  // NOTE: This file is stable and usually should not be modified.
  // It is important that all functionality in this file is preserved, and should only be modified if explicitly requested.

  import { cn } from '$lib/utils';
  import { browser } from '$app/environment';
  import { Dialog as DialogPrimitive } from 'bits-ui';
  import { generateSecretKey, nip19 } from 'nostr-tools';
  import { loginWithNsec, publishProfile } from '$lib/stores/auth';

  interface Props {
    isOpen: boolean;
    onClose: () => void;
    onComplete?: () => void;
  }

  let { isOpen = $bindable(), onClose, onComplete }: Props = $props();

  type Step = 'welcome' | 'generate' | 'download' | 'profile' | 'done';

  let step = $state<Step>('welcome');
  let isLoading = $state(false);
  let nsec = $state('');
  let showSparkles = $state(false);
  let keySecured = $state<'none' | 'downloaded'>('none');
  let profileData = $state({
    name: '',
    about: '',
    picture: ''
  });
  let isPublishing = $state(false);
  let isUploading = $state(false);
  let avatarFileInputRef = $state<HTMLInputElement | undefined>(undefined);

  import { uploadFileWithRetry } from '$lib/stores/upload.svelte';

  const sanitizeFilename = (filename: string) => {
    return filename.replace(/[^a-z0-9_.-]/gi, '_');
  };

  // Generate a proper nsec key using nostr-tools
  const generateKey = () => {
    isLoading = true;
    showSparkles = true;

    // Add a dramatic pause for the key generation effect
    setTimeout(() => {
      try {
        // Generate a new secret key
        const sk = generateSecretKey();

        // Convert to nsec format
        nsec = nip19.nsecEncode(sk);
        step = 'download';

        console.log('Your Secret Key is Ready!');
      } catch {
        alert('Failed to generate key. Please try again.');
      } finally {
        isLoading = false;
        showSparkles = false;
      }
    }, 2000);
  };

  const downloadKey = () => {
    if (!browser) return;

    try {
      // Create a blob with the key text
      const blob = new Blob([nsec], { type: 'text/plain; charset=utf-8' });
      const url = globalThis.URL.createObjectURL(blob);

      // Sanitize filename
      const filename = sanitizeFilename('nostr-nsec-key.txt');

      // Create a temporary link element and trigger download
      const a = document.createElement('a');
      a.href = url;
      a.download = filename;
      a.style.display = 'none';
      document.body.appendChild(a);
      a.click();

      // Clean up immediately
      globalThis.URL.revokeObjectURL(url);
      document.body.removeChild(a);

      // Mark as secured
      keySecured = 'downloaded';

      console.log('Secret Key Saved!');
    } catch {
      alert('Download failed. Could not download the key file. Please copy it manually.');
    }
  };

  const finishKeySetup = () => {
    try {
      loginWithNsec(nsec);
      step = 'profile';
    } catch (e: unknown) {
      const error = e as Error;
      console.error('Login failed:', error);
      alert('Login Failed. Failed to login with the generated key. Please try again.');
    }
  };

  const handleAvatarUpload = async (e: Event) => {
    const target = e.target as HTMLInputElement;
    const file = target.files?.[0];
    if (!file) return;

    // Reset file input
    target.value = '';

    // Validate file type
    if (!file.type.startsWith('image/')) {
      alert('Invalid file type. Please select an image file for your avatar.');
      return;
    }

    // Validate file size (max 5MB)
    if (file.size > 5 * 1024 * 1024) {
      alert('File too large. Avatar image must be smaller than 5MB.');
      return;
    }

    isUploading = true;
    try {
      const result = await uploadFileWithRetry(file);
      profileData.picture = result.url;
      console.log('Avatar uploaded successfully:', result.url);
    } catch (error) {
      console.error('Avatar upload failed:', error);
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      alert(`Upload failed: ${errorMessage}. Please try again.`);
    } finally {
      isUploading = false;
    }
  };

  const finishSignup = async (skipProfile = false) => {
    if (browser) {
      // Mark signup completion time for fallback welcome modal
      localStorage.setItem('signup_completed', Date.now().toString());
    }

    isPublishing = true;

    try {
      // Publish profile if user provided information
      if (!skipProfile && (profileData.name || profileData.about || profileData.picture)) {
        const metadata: Record<string, string> = {};
        if (profileData.name) metadata.name = profileData.name;
        if (profileData.about) metadata.about = profileData.about;
        if (profileData.picture) metadata.picture = profileData.picture;

        await publishProfile(metadata);
      }

      // Close signup and show welcome modal
      onClose();
      if (onComplete) {
        // Add a longer delay to ensure login state has fully propagated
        setTimeout(() => {
          onComplete();
        }, 600);
      } else {
        // Fallback for when used without onComplete
        step = 'done';
        setTimeout(() => {
          onClose();
        }, 3000);
      }
    } catch (e: unknown) {
      const error = e as Error;
      console.error('Profile setup failed:', error);
      alert('Profile Setup Failed. Your account was created but profile setup failed. You can update it later.');

      // Still proceed to completion even if profile failed
      onClose();
      if (onComplete) {
        setTimeout(() => {
          onComplete();
        }, 600);
      }
    } finally {
      isPublishing = false;
    }
  };

  // Reset state when dialog opens
  $effect(() => {
    if (isOpen) {
      step = 'welcome';
      isLoading = false;
      nsec = '';
      showSparkles = false;
      keySecured = 'none';
      profileData = { name: '', about: '', picture: '' };
    }
  });
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
        class={cn("fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 max-w-[95vw] sm:max-w-md max-h-[90vh] p-0 overflow-hidden rounded-2xl flex flex-col bg-background shadow-lg")}
        onclick={(e) => e.stopPropagation()}
        onkeydown={(e) => e.stopPropagation()}
        role="dialog"
        tabindex="-1"
      >
      <!-- Header -->
      <div class="px-6 pt-6 pb-1 relative flex-shrink-0">
        <h2 class="font-semibold text-center text-lg">
          {#if step === 'welcome'}
            Create Your Account
          {:else if step === 'generate'}
            Generating Your Key
          {:else if step === 'download'}
            Secret Key
          {:else if step === 'profile'}
            Create Your Profile
          {:else}
            Welcome!
          {/if}
        </h2>
      </div>

      <!-- Content -->
      <div class="px-6 pt-2 pb-4 space-y-4 overflow-y-scroll flex-1">
        <!-- Welcome Step -->
        {#if step === 'welcome'}
          <div class="text-center space-y-4">
            <div class="relative p-6 rounded-2xl bg-gradient-to-br from-blue-50 to-indigo-100 dark:from-blue-950/50 dark:to-indigo-950/50">
              <div class="flex justify-center items-center space-x-4 mb-3">
                <svg class="w-12 h-12 text-blue-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M18 9v3m0 0v3m0-3h3m-3 0h-3m-2-5a4 4 0 11-8 0 4 4 0 018 0zM3 20a6 6 0 0112 0v1H3v-1z" />
                </svg>
              </div>
              <div class="grid grid-cols-1 gap-2 text-sm">
                <div class="flex items-center justify-center gap-2 text-blue-700 dark:text-blue-300">
                  Decentralized and censorship-resistant
                </div>
                <div class="flex items-center justify-center gap-2 text-blue-700 dark:text-blue-300">
                  You are in control of your data
                </div>
                <div class="flex items-center justify-center gap-2 text-blue-700 dark:text-blue-300">
                  Join a global network
                </div>
              </div>
            </div>
            <button
              type="button"
              class="w-full rounded-full py-6 text-lg font-semibold bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-700 hover:to-indigo-700 transform transition-all duration-200 hover:scale-105 shadow-lg text-white"
              onclick={() => (step = 'generate')}
            >
              Get Started
            </button>
          </div>
        {/if}

        <!-- Generate Step -->
        {#if step === 'generate'}
          <div class="text-center space-y-4">
            <div class="relative p-6 rounded-2xl bg-gradient-to-br from-blue-50 to-purple-100 dark:from-blue-950/50 dark:to-purple-950/50 overflow-hidden">
              {#if isLoading}
                <div class="space-y-3">
                  <svg class="w-20 h-20 text-primary mx-auto animate-pulse" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
                  </svg>
                  <p class="text-lg font-semibold text-primary">
                    Generating your secret key...
                  </p>
                </div>
              {:else}
                <div class="space-y-3">
                  <svg class="w-20 h-20 text-primary mx-auto" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M15 7a2 2 0 012 2m4 0a6 6 0 01-7.743 5.743L11 17H9v2H7v2H4a1 1 0 01-1-1v-2.586a1 1 0 01.293-.707l5.964-5.964A6 6 0 1121 9z" />
                  </svg>
                  <p class="text-lg font-semibold">Ready to generate your secret key?</p>
                  <p class="text-sm text-muted-foreground">
                    This key will be your password to access applications within the Nostr network.
                  </p>
                </div>
              {/if}
            </div>
            {#if !isLoading}
              <button
                type="button"
                class="w-full rounded-full py-6 text-lg font-semibold bg-gradient-to-r from-purple-600 to-blue-600 hover:from-purple-700 hover:to-blue-700 transform transition-all duration-200 hover:scale-105 shadow-lg text-white"
                onclick={generateKey}
                disabled={isLoading}
              >
                Generate My Secret Key
              </button>
            {/if}
          </div>
        {/if}

        <!-- Download Step -->
        {#if step === 'download'}
          <div class="text-center space-y-4">
            <div class="relative p-6 rounded-2xl bg-gradient-to-br from-blue-50 to-indigo-100 dark:from-blue-950/50 dark:to-indigo-950/50">
              <p class="text-base font-semibold mb-2">Your secret key has been generated!</p>
              <div class="p-3 bg-amber-100 dark:from-amber-950/40 rounded-lg border-2 border-amber-300 dark:border-amber-700">
                <p class="text-xs text-amber-800 dark:text-amber-200 font-bold mb-1">Important Warning</p>
                <p class="text-xs text-red-700 dark:text-amber-300 italic">
                  This key is your primary and only means of accessing your account. Store it safely and securely.
                </p>
              </div>
            </div>

            <button
              type="button"
              class="w-full p-3 rounded-lg border {keySecured === 'downloaded' ? 'ring-2 ring-green-500 bg-green-50 dark:bg-green-950/20' : 'hover:bg-primary/5'}"
              onclick={downloadKey}
            >
              <div class="flex items-center gap-3">
                <div class="p-1.5 rounded-lg {keySecured === 'downloaded' ? 'bg-green-100 dark:bg-green-900' : 'bg-primary/10'}">
                  {#if keySecured === 'downloaded'}
                    <svg class="w-4 h-4 text-green-600" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
                    </svg>
                  {:else}
                    <svg class="w-4 h-4 text-primary" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-4l-4 4m0 0l-4-4m4 4V4" />
                    </svg>
                  {/if}
                </div>
                <div class="flex-1 text-left">
                  <div class="font-medium text-sm">Download as File</div>
                  <div class="text-xs text-muted-foreground">Save as nostr-nsec-key.txt file</div>
                </div>
                {#if keySecured === 'downloaded'}
                  <div class="text-xs font-medium text-green-600">✓ Downloaded</div>
                {/if}
              </div>
            </button>

            <button
              type="button"
              class="w-full rounded-full py-4 text-base font-semibold transform transition-all duration-200 shadow-lg {keySecured === 'downloaded' ? 'bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-700 hover:to-indigo-700 hover:scale-105 text-white' : 'bg-gradient-to-r from-blue-600/60 to-indigo-600/60 text-muted cursor-not-allowed'}"
              onclick={finishKeySetup}
              disabled={keySecured !== 'downloaded'}
            >
              {keySecured === 'none' ? 'Please download your key first' : 'My Key is Safe - Continue'}
            </button>
          </div>
        {/if}

        <!-- Profile Step -->
        {#if step === 'profile'}
          <div class="text-center space-y-4">
            <div class="relative p-6 rounded-2xl bg-gradient-to-br from-blue-50 to-indigo-100 dark:from-blue-950/50 dark:to-indigo-950/50">
              <p class="text-base font-semibold">Almost there! Let's set up your profile</p>
              <p class="text-sm text-muted-foreground">Your profile is your identity on Nostr.</p>
            </div>

            {#if isPublishing}
              <div class="p-4 rounded-xl bg-gradient-to-r from-blue-50 to-indigo-50 dark:from-blue-950/30 dark:to-indigo-950/30 border border-blue-200 dark:border-blue-800">
                <div class="flex items-center justify-center gap-3">
                  <div class="w-5 h-5 border-2 border-blue-600 border-t-transparent rounded-full animate-spin"></div>
                  <span class="text-sm font-medium text-blue-700 dark:text-blue-300">Publishing your profile...</span>
                </div>
              </div>
            {/if}

            <div class="space-y-4 text-left {isPublishing ? 'opacity-50 pointer-events-none' : ''}">
              <div class="space-y-2">
                <label for="profile-name" class="text-sm font-medium">Display Name</label>
                <input
                  id="profile-name"
                  type="text"
                  bind:value={profileData.name}
                  placeholder="Your name"
                  class="w-full px-3 py-2 border rounded-lg bg-background"
                  disabled={isPublishing}
                />
              </div>

              <div class="space-y-2">
                <label for="profile-about" class="text-sm font-medium">Bio</label>
                <textarea
                  id="profile-about"
                  bind:value={profileData.about}
                  placeholder="Tell others about yourself..."
                  class="w-full px-3 py-2 border rounded-lg bg-background resize-none"
                  rows="3"
                  disabled={isPublishing}
                ></textarea>
              </div>

              <div class="space-y-2">
                <label for="profile-picture" class="text-sm font-medium">Avatar</label>
                <div class="flex gap-2">
                  <input
                    id="profile-picture"
                    type="text"
                    bind:value={profileData.picture}
                    placeholder="https://example.com/your-avatar.jpg"
                    class="w-full px-3 py-2 border rounded-lg bg-background flex-1"
                    disabled={isPublishing}
                  />
                  <input
                    type="file"
                    accept="image/*"
                    class="hidden"
                    bind:this={avatarFileInputRef}
                    onchange={handleAvatarUpload}
                  />
                  <button
                    type="button"
                    onclick={() => avatarFileInputRef?.click()}
                    disabled={isUploading || isPublishing}
                    class="px-3 py-2 border rounded-lg hover:bg-accent disabled:opacity-50"
                  >
                    {#if isUploading}
                      <div class="w-4 h-4 border-2 border-current border-t-transparent rounded-full animate-spin"></div>
                    {:else}
                      📤
                    {/if}
                  </button>
                </div>
              </div>
            </div>

            <div class="space-y-3">
              <button
                type="button"
                class="w-full rounded-full py-4 text-base font-semibold bg-gradient-to-r from-blue-600 to-indigo-600 hover:from-blue-700 hover:to-indigo-700 transform transition-all duration-200 hover:scale-105 shadow-lg disabled:opacity-50 disabled:cursor-not-allowed disabled:transform-none text-white"
                onclick={() => finishSignup(false)}
                disabled={isPublishing || isUploading}
              >
                {isPublishing ? 'Creating Profile...' : 'Create Profile & Finish'}
              </button>

              <button
                type="button"
                class="w-full rounded-full py-3 border disabled:opacity-50 disabled:cursor-not-allowed"
                onclick={() => finishSignup(true)}
                disabled={isPublishing || isUploading}
              >
                {isPublishing ? 'Setting up account...' : 'Skip for now'}
              </button>
            </div>
          </div>
        {/if}
      </div>
    </div>
    </div>
  </DialogPrimitive.Portal>
{/if}
