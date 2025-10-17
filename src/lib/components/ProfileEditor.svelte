<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
	import { load } from '@welshman/net';
	import { publishProfile, currentPubkey } from '$lib/stores/auth';
	import { useToast } from '$lib/stores/toast.svelte';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Textarea } from '$lib/components/ui/textarea';
	import { Label } from '$lib/components/ui/label';

	interface Props {
		isOpen: boolean;
		onClose: () => void;
		class?: string;
	}

	let { isOpen = $bindable(), onClose, class: className = '' }: Props = $props();

	const queryClient = useQueryClient();
	const toast = useToast();

	// Fetch current profile
	const profileQuery = createQuery(() => ({
		queryKey: ['my-profile', $currentPubkey],
		queryFn: async () => {
			if (!$currentPubkey) return {};

			const profiles = await load({
				relays: [],
				filters: [
					{
						kinds: [0],
						authors: [$currentPubkey],
						limit: 1
					}
				]
			});

			if (profiles.length > 0) {
				try {
					return JSON.parse(profiles[0].content);
				} catch {
					return {};
				}
			}
			return {};
		},
		enabled: !!$currentPubkey
	}));

	// Form state
	let name = $state('');
	let displayName = $state('');
	let about = $state('');
	let picture = $state('');
	let banner = $state('');
	let website = $state('');
	let nip05 = $state('');
	let lud16 = $state('');

	// Initialize form when profile loads
	$effect(() => {
		if ($profileQuery.data) {
			const profile = $profileQuery.data;
			name = profile.name || '';
			displayName = profile.display_name || '';
			about = profile.about || '';
			picture = profile.picture || '';
			banner = profile.banner || '';
			website = profile.website || '';
			nip05 = profile.nip05 || '';
			lud16 = profile.lud16 || '';
		}
	});

	// Save mutation
	const saveMutation = createMutation({
		mutationFn: async () => {
			const profile = {
				name: name.trim(),
				display_name: displayName.trim(),
				about: about.trim(),
				picture: picture.trim(),
				banner: banner.trim(),
				website: website.trim(),
				nip05: nip05.trim(),
				lud16: lud16.trim()
			};

			// Remove empty fields
			Object.keys(profile).forEach((key) => {
				if (profile[key as keyof typeof profile] === '') {
					delete profile[key as keyof typeof profile];
				}
			});

			await publishProfile(profile);
			return profile;
		},
		onSuccess: (data) => {
			// Invalidate profile queries
			queryClient.invalidateQueries({ queryKey: ['my-profile'] });
			queryClient.invalidateQueries({ queryKey: ['profile', $currentPubkey] });

			toast.success('Profile updated successfully!');
			onClose();
		},
		onError: (error) => {
			toast.error('Failed to update profile');
			console.error('Profile update error:', error);
		}
	});

	function handleSubmit() {
		$saveMutation.mutate();
	}

	function handleCancel() {
		onClose();
	}
</script>

<Dialog.Root bind:open={isOpen}>
	<Dialog.Content class="sm:max-w-[600px] max-h-[80vh] overflow-y-auto {className}">
		<Dialog.Header>
			<Dialog.Title>Edit Profile</Dialog.Title>
			<Dialog.Description>
				Update your Nostr profile information. This will be published to your configured relays.
			</Dialog.Description>
		</Dialog.Header>

		<div class="space-y-4">
			{#if $profileQuery.isLoading}
				<p class="text-sm text-muted-foreground">Loading profile...</p>
			{:else}
				<!-- Display Name -->
				<div class="space-y-2">
					<Label for="display-name">Display Name</Label>
					<Input
						id="display-name"
						bind:value={displayName}
						placeholder="Your display name"
						disabled={$saveMutation.isPending}
					/>
				</div>

				<!-- Username -->
				<div class="space-y-2">
					<Label for="name">Username</Label>
					<Input
						id="name"
						bind:value={name}
						placeholder="username"
						disabled={$saveMutation.isPending}
					/>
				</div>

				<!-- About -->
				<div class="space-y-2">
					<Label for="about">About</Label>
					<Textarea
						id="about"
						bind:value={about}
						placeholder="Tell us about yourself"
						class="min-h-[100px]"
						disabled={$saveMutation.isPending}
					/>
				</div>

				<!-- Profile Picture URL -->
				<div class="space-y-2">
					<Label for="picture">Profile Picture URL</Label>
					<Input
						id="picture"
						bind:value={picture}
						placeholder="https://example.com/avatar.jpg"
						type="url"
						disabled={$saveMutation.isPending}
					/>
				</div>

				<!-- Banner URL -->
				<div class="space-y-2">
					<Label for="banner">Banner URL</Label>
					<Input
						id="banner"
						bind:value={banner}
						placeholder="https://example.com/banner.jpg"
						type="url"
						disabled={$saveMutation.isPending}
					/>
				</div>

				<!-- Website -->
				<div class="space-y-2">
					<Label for="website">Website</Label>
					<Input
						id="website"
						bind:value={website}
						placeholder="https://yourwebsite.com"
						type="url"
						disabled={$saveMutation.isPending}
					/>
				</div>

				<!-- NIP-05 -->
				<div class="space-y-2">
					<Label for="nip05">NIP-05 Identifier</Label>
					<Input
						id="nip05"
						bind:value={nip05}
						placeholder="name@domain.com"
						disabled={$saveMutation.isPending}
					/>
					<p class="text-xs text-muted-foreground">
						Your Nostr address for verification (e.g., name@domain.com)
					</p>
				</div>

				<!-- Lightning Address -->
				<div class="space-y-2">
					<Label for="lud16">Lightning Address</Label>
					<Input
						id="lud16"
						bind:value={lud16}
						placeholder="name@getalby.com"
						disabled={$saveMutation.isPending}
					/>
					<p class="text-xs text-muted-foreground">
						Your Lightning address for receiving zaps (e.g., name@getalby.com)
					</p>
				</div>
			{/if}
		</div>

		<Dialog.Footer class="gap-2">
			<Button variant="outline" onclick={handleCancel} disabled={$saveMutation.isPending}>
				Cancel
			</Button>
			<Button onclick={handleSubmit} disabled={$saveMutation.isPending || $profileQuery.isLoading}>
				{#if $saveMutation.isPending}
					Saving...
				{:else}
					Save Changes
				{/if}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
