<script lang="ts">
  import { createQuery, useQueryClient } from '@tanstack/svelte-query';
  import { z } from 'zod';
  import { currentPubkey, publishProfile } from '$lib/stores/auth';
  import { fetchAuthor, type AuthorData } from '$lib/stores/author.svelte';
  import { toastSuccess, toastError } from '$lib/stores/toast.svelte';
  import { uploadFileWithRetry } from '$lib/stores/upload.svelte';

  const queryClient = useQueryClient();

  // Nostr metadata schema (from NIP-01)
  const metadataSchema = z.object({
    name: z.string().optional(),
    about: z.string().optional(),
    picture: z.string().url().optional().or(z.literal('')),
    banner: z.string().url().optional().or(z.literal('')),
    website: z.string().url().optional().or(z.literal('')),
    nip05: z.string().optional(),
    bot: z.boolean().optional(),
  });

  type NostrMetadata = z.infer<typeof metadataSchema>;

  // Form state
  let formData = $state<NostrMetadata>({
    name: '',
    about: '',
    picture: '',
    banner: '',
    website: '',
    nip05: '',
    bot: false,
  });

  let isPending = $state(false);
  let isUploading = $state(false);
  let errors = $state<Partial<Record<keyof NostrMetadata, string>>>({});

  // File input refs
  let pictureInputRef: HTMLInputElement;
  let bannerInputRef: HTMLInputElement;

  // Query current user metadata using Welshman
  // @ts-expect-error - TanStack Query in Svelte requires createQuery to be called within component context
  const userQuery = createQuery<AuthorData>(() => ({
    queryKey: ['author', $currentPubkey ?? ''],
    queryFn: ({ signal }) => fetchAuthor($currentPubkey ?? '', signal),
    enabled: !!$currentPubkey,
    staleTime: 30000 // 30 seconds
  }));

  // Load user metadata into form when available
  $effect(() => {
    const userData = $userQuery.data as AuthorData | undefined;
    if (userData?.metadata) {
      // Cast to any to access bot field which isn't in Welshman's Profile type
      const metadata = userData.metadata as any;
      formData = {
        name: userData.metadata.name || '',
        about: userData.metadata.about || '',
        picture: userData.metadata.picture || '',
        banner: userData.metadata.banner || '',
        website: userData.metadata.website || '',
        nip05: userData.metadata.nip05 || '',
        bot: metadata.bot || false,
      };
    }
  });

  // File upload handler
  async function uploadFile(file: File, field: 'picture' | 'banner') {
    isUploading = true;
    try {
      const result = await uploadFileWithRetry(file);
      formData[field] = result.url;
      console.log(`${field} uploaded successfully:`, result.url);
      toastSuccess('Upload successful', `Your ${field === 'picture' ? 'profile picture' : 'banner'} has been uploaded.`);
    } catch (error) {
      console.error(`Failed to upload ${field}:`, error);
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      toastError('Upload failed', `Failed to upload ${field === 'picture' ? 'profile picture' : 'banner'}: ${errorMessage}`);
    } finally {
      isUploading = false;
    }
  }

  // Form submission handler
  async function handleSubmit(event: Event) {
    event.preventDefault();

    // Validate form
    const result = metadataSchema.safeParse(formData);
    if (!result.success) {
      errors = result.error.flatten().fieldErrors as any;
      return;
    }

    errors = {};
    isPending = true;

    try {
      // Clean up empty values
      const cleanData: any = {};
      for (const [key, value] of Object.entries(formData)) {
        if (value !== '' && value !== undefined) {
          cleanData[key] = value;
        }
      }

      // Publish metadata event (kind 0) using Welshman
      await publishProfile(cleanData);

      // Invalidate queries to refresh data
      await queryClient.invalidateQueries({ queryKey: ['author'] });

      toastSuccess('Profile updated!', 'Your profile has been published to the network.');
    } catch (error) {
      console.error('Failed to update profile:', error);
      toastError('Failed to update profile', (error as Error).message || 'Please try again.');
    } finally {
      isPending = false;
    }
  }

  function handleFileSelect(event: Event, field: 'picture' | 'banner') {
    const target = event.target as HTMLInputElement;
    const file = target.files?.[0];
    if (file) {
      uploadFile(file, field);
    }
  }
</script>

<div class="max-w-2xl mx-auto p-6">
  <h2 class="text-2xl font-bold mb-6">Edit Profile</h2>

  <form onsubmit={handleSubmit} class="space-y-6">
    <!-- Name -->
    <div class="space-y-2">
      <label for="name" class="block text-sm font-medium">
        Name
      </label>
      <input
        id="name"
        type="text"
        bind:value={formData.name}
        placeholder="Your name"
        class="w-full px-3 py-2 border rounded-md bg-background"
      />
      <p class="text-sm text-muted-foreground">
        This is your display name that will be displayed to others.
      </p>
      {#if errors.name}
        <p class="text-sm text-destructive">{errors.name}</p>
      {/if}
    </div>

    <!-- About -->
    <div class="space-y-2">
      <label for="about" class="block text-sm font-medium">
        Bio
      </label>
      <textarea
        id="about"
        bind:value={formData.about}
        placeholder="Tell others about yourself"
        rows="4"
        class="w-full px-3 py-2 border rounded-md bg-background resize-none"
      ></textarea>
      <p class="text-sm text-muted-foreground">
        A short description about yourself.
      </p>
      {#if errors.about}
        <p class="text-sm text-destructive">{errors.about}</p>
      {/if}
    </div>

    <!-- Picture and Banner -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
      <!-- Picture -->
      <div class="space-y-2">
        <label for="picture" class="block text-sm font-medium">
          Profile Picture
        </label>
        <input
          id="picture"
          type="text"
          bind:value={formData.picture}
          placeholder="https://example.com/profile.jpg"
          class="w-full px-3 py-2 border rounded-md bg-background"
        />
        <div class="flex items-center gap-2">
          <input
            bind:this={pictureInputRef}
            type="file"
            accept="image/*"
            onchange={(e) => handleFileSelect(e, 'picture')}
            class="hidden"
          />
          <button
            type="button"
            onclick={() => pictureInputRef?.click()}
            class="px-3 py-1.5 text-sm border rounded-md hover:bg-accent"
          >
            Upload Image
          </button>
          {#if formData.picture}
            <div class="h-10 w-10 rounded overflow-hidden">
              <img
                src={formData.picture}
                alt="Profile preview"
                class="h-full w-full object-cover"
              />
            </div>
          {/if}
        </div>
        <p class="text-sm text-muted-foreground">
          URL to your profile picture. You can upload an image or provide a URL.
        </p>
      </div>

      <!-- Banner -->
      <div class="space-y-2">
        <label for="banner" class="block text-sm font-medium">
          Banner Image
        </label>
        <input
          id="banner"
          type="text"
          bind:value={formData.banner}
          placeholder="https://example.com/banner.jpg"
          class="w-full px-3 py-2 border rounded-md bg-background"
        />
        <div class="flex items-center gap-2">
          <input
            bind:this={bannerInputRef}
            type="file"
            accept="image/*"
            onchange={(e) => handleFileSelect(e, 'banner')}
            class="hidden"
          />
          <button
            type="button"
            onclick={() => bannerInputRef?.click()}
            class="px-3 py-1.5 text-sm border rounded-md hover:bg-accent"
          >
            Upload Image
          </button>
          {#if formData.banner}
            <div class="h-10 w-24 rounded overflow-hidden">
              <img
                src={formData.banner}
                alt="Banner preview"
                class="h-full w-full object-cover"
              />
            </div>
          {/if}
        </div>
        <p class="text-sm text-muted-foreground">
          URL to a wide banner image for your profile. You can upload an image or provide a URL.
        </p>
      </div>
    </div>

    <!-- Website and NIP-05 -->
    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
      <!-- Website -->
      <div class="space-y-2">
        <label for="website" class="block text-sm font-medium">
          Website
        </label>
        <input
          id="website"
          type="text"
          bind:value={formData.website}
          placeholder="https://yourwebsite.com"
          class="w-full px-3 py-2 border rounded-md bg-background"
        />
        <p class="text-sm text-muted-foreground">
          Your personal website or social media link.
        </p>
      </div>

      <!-- NIP-05 -->
      <div class="space-y-2">
        <label for="nip05" class="block text-sm font-medium">
          NIP-05 Identifier
        </label>
        <input
          id="nip05"
          type="text"
          bind:value={formData.nip05}
          placeholder="you@example.com"
          class="w-full px-3 py-2 border rounded-md bg-background"
        />
        <p class="text-sm text-muted-foreground">
          Your verified Nostr identifier.
        </p>
      </div>
    </div>

    <!-- Bot Account -->
    <div class="flex items-center justify-between rounded-lg border p-4">
      <div class="space-y-0.5">
        <label for="bot" class="text-base font-medium">
          Bot Account
        </label>
        <p class="text-sm text-muted-foreground">
          Mark this account as automated or a bot.
        </p>
      </div>
      <input
        id="bot"
        type="checkbox"
        bind:checked={formData.bot}
        class="h-5 w-5"
      />
    </div>

    <!-- Submit Button -->
    <button
      type="submit"
      disabled={isPending || isUploading}
      class="w-full md:w-auto px-4 py-2 bg-primary text-primary-foreground rounded-md hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed"
    >
      {#if isPending || isUploading}
        <span class="inline-block animate-spin mr-2">⏳</span>
      {/if}
      Save Profile
    </button>
  </form>
</div>

<style>
  /* Additional styles can go here if needed */
</style>
