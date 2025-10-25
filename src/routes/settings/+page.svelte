<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { currentUser, logout } from '$lib/stores/auth';
	import { appConfig, type Theme, presetRelays } from '$lib/stores/appStore';
	import { Settings, User, Moon, Sun, Monitor, LogOut, Palette, Zap } from 'lucide-svelte';
	import * as Card from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';
	import { Label } from '$lib/components/ui/label';
	import { Input } from '$lib/components/ui/input';
	import { nip19 } from 'nostr-tools';
	import { goto } from '$app/navigation';
	import { cn } from '$lib/utils';

	// Reactive values
	let npub = $derived.by(() => {
		if (!$currentUser) return '';
		return nip19.npubEncode($currentUser.pubkey);
	});

	function handleThemeChange(theme: Theme) {
		appConfig.updateTheme(theme);
	}

	function handleRelayChange(url: string) {
		appConfig.updateRelayUrl(url);
	}

	function handleLogout() {
		logout();
		goto('/');
	}

	function copyToClipboard(text: string) {
		navigator.clipboard.writeText(text);
	}

	const themeOptions = [
		{ value: 'light' as Theme, label: 'Light', icon: Sun },
		{ value: 'dark' as Theme, label: 'Dark', icon: Moon },
		{ value: 'system' as Theme, label: 'System', icon: Monitor }
	];
</script>

<MainLayout>
	{#snippet sidebar()}
		<AppSidebar />
	{/snippet}

	{#snippet rightPanel()}
		<RightSidebar />
	{/snippet}

	{#snippet children()}
		<div class="min-h-screen">
			<!-- Header -->
			<div class="sticky top-0 z-10 bg-background/80 backdrop-blur-sm border-b border-border">
				<div class="px-4 pt-3">
					<h1 class="text-xl font-bold px-4 py-3 flex items-center gap-2">
						<Settings class="w-6 h-6" />
						Settings
					</h1>
				</div>
			</div>

			<!-- Content -->
			<div class="p-6 max-w-2xl mx-auto space-y-6">
				{#if !$currentUser}
					<!-- Not logged in -->
					<div class="flex flex-col items-center justify-center py-20 px-4 text-center">
						<Settings class="w-16 h-16 mb-4 text-muted-foreground" />
						<h2 class="text-2xl font-bold mb-2">Settings</h2>
						<p class="text-muted-foreground max-w-sm mb-6">
							Log in to access your account settings and preferences.
						</p>
					</div>
				{:else}
					<!-- Account Section -->
					<Card.Root>
						<Card.Header>
							<Card.Title class="flex items-center gap-2">
								<User class="w-5 h-5" />
								Account
							</Card.Title>
							<Card.Description>Your Nostr identity and keys</Card.Description>
						</Card.Header>
						<Card.Content class="space-y-4">
							<div class="space-y-2">
								<Label for="npub">Public Key (npub)</Label>
								<div class="flex gap-2">
									<Input id="npub" value={npub} readonly class="font-mono text-sm" />
									<Button
										variant="outline"
										size="sm"
										onclick={() => copyToClipboard(npub)}
									>
										Copy
									</Button>
								</div>
								<p class="text-xs text-muted-foreground">
									Your public identifier on Nostr. Safe to share publicly.
								</p>
							</div>

							<div class="pt-4">
								<Button variant="destructive" onclick={handleLogout} class="w-full">
									<LogOut class="w-4 h-4 mr-2" />
									Log Out
								</Button>
							</div>
						</Card.Content>
					</Card.Root>

					<!-- Appearance Section -->
					<Card.Root>
						<Card.Header>
							<Card.Title class="flex items-center gap-2">
								<Palette class="w-5 h-5" />
								Appearance
							</Card.Title>
							<Card.Description>Customize how nostr.blue looks</Card.Description>
						</Card.Header>
						<Card.Content class="space-y-4">
							<div class="space-y-2">
								<Label>Theme</Label>
								<div class="grid grid-cols-3 gap-2">
									{#each themeOptions as option}
										{@const Icon = option.icon}
										<Button
											variant={$appConfig.theme === option.value ? 'default' : 'outline'}
											class={cn('flex flex-col gap-2 h-auto py-3')}
											onclick={() => handleThemeChange(option.value)}
										>
											<Icon class="w-5 h-5" />
											<span class="text-xs">{option.label}</span>
										</Button>
									{/each}
								</div>
								<p class="text-xs text-muted-foreground">
									Choose your preferred color scheme
								</p>
							</div>
						</Card.Content>
					</Card.Root>

					<!-- Relays Section -->
					<Card.Root>
						<Card.Header>
							<Card.Title class="flex items-center gap-2">
								<Zap class="w-5 h-5" />
								Relays
							</Card.Title>
							<Card.Description>Configure your relay connections</Card.Description>
						</Card.Header>
						<Card.Content class="space-y-4">
							<div class="space-y-2">
								<Label>Default Relay</Label>
								<div class="space-y-2">
									{#each presetRelays as relay}
										<Button
											variant={$appConfig.relayUrl === relay.url ? 'default' : 'outline'}
											class="w-full justify-start h-auto py-3"
											onclick={() => handleRelayChange(relay.url)}
										>
											<div class="flex flex-col items-start">
												<span class="font-medium">{relay.name}</span>
												<span class="text-xs opacity-70">{relay.url}</span>
											</div>
										</Button>
									{/each}
								</div>
								<p class="text-xs text-muted-foreground">
									Your primary relay for reading and publishing events
								</p>
							</div>

							<div class="rounded-lg bg-muted p-4 text-sm">
								<p class="font-medium mb-2">💡 About Relays</p>
								<p class="text-muted-foreground">
									nostr.blue uses NIP-65 (Relay List Metadata) to automatically discover and
									use the best relays for each user. The default relay is used as a fallback
									when no user-specific relays are found.
								</p>
							</div>
						</Card.Content>
					</Card.Root>

					<!-- About Section -->
					<Card.Root>
						<Card.Header>
							<Card.Title>About nostr.blue</Card.Title>
						</Card.Header>
						<Card.Content class="space-y-2 text-sm text-muted-foreground">
							<p>A modern Nostr client built with Svelte 5 and Welshman.</p>
							<div class="flex gap-4 pt-2">
								<a
									href="https://github.com/nostr-protocol/nips"
									target="_blank"
									rel="noopener noreferrer"
									class="text-primary hover:underline"
								>
									Nostr NIPs
								</a>
								<a
									href="https://github.com/coracle-social/welshman"
									target="_blank"
									rel="noopener noreferrer"
									class="text-primary hover:underline"
								>
									Welshman Toolkit
								</a>
							</div>
						</Card.Content>
					</Card.Root>
				{/if}
			</div>
		</div>
	{/snippet}
</MainLayout>
