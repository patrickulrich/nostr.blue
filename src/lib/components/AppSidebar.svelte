<script lang="ts">
	import {
		Home,
		Compass,
		User,
		Settings,
		PenSquare,
		Bell,
		Mail,
		List,
		Bookmark,
		Users,
		MoreHorizontal,
		Video,
		Calendar,
		Music,
		Zap
	} from 'lucide-svelte';
	import { page } from '$app/stores';
	import { goto, invalidateAll } from '$app/navigation';
	import { Button } from '$lib/components/ui/button';
	import LoginArea from '$lib/components/auth/LoginArea.svelte';
	import NoteComposer from '$lib/components/NoteComposer.svelte';
	import * as Popover from '$lib/components/ui/popover';
	import * as Dialog from '$lib/components/ui/dialog';
	import { currentUser } from '$lib/stores/auth';
	import { nip19 } from 'nostr-tools';
	import { cn } from '$lib/utils';

	let composeOpen = $state(false);
	let moreOpen = $state(false);
	let mounted = $state(false);

	let profilePath = $derived($currentUser ? `/${nip19.npubEncode($currentUser.pubkey)}` : '/');

	// Mount on client side only to avoid hydration issues with Popover
	$effect(() => {
		mounted = true;
	});

	function handleHomeClick(e: MouseEvent) {
		// If already on home page, scroll to top and refresh feed
		if ($page.url.pathname === '/') {
			e.preventDefault();
			window.scrollTo({ top: 0, behavior: 'smooth' });
			// Invalidate all queries to trigger refresh
			invalidateAll();
		}
	}

	interface NavItemProps {
		href: string;
		icon: any;
		label: string;
	}

	function isActive(href: string): boolean {
		return $page.url.pathname === href;
	}
</script>

<div class="flex flex-col h-full justify-between">
	<div class="flex flex-col gap-2">
		<!-- Logo/Brand -->
		<div class="px-4 py-3 mb-2">
			<a href="/" onclick={handleHomeClick}>
				<div
					class="w-12 h-12 rounded-full bg-blue-500 flex items-center justify-center text-white font-bold text-xl hover:bg-blue-600 transition-colors"
				>
					N
				</div>
			</a>
		</div>

		<!-- Navigation -->
		<nav class="flex flex-col gap-1">
			<!-- Home - with special click handling for refresh -->
			<a href="/" onclick={handleHomeClick}>
				<Button
					variant="ghost"
					class={cn(
						'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
						isActive('/') && 'font-bold'
					)}
				>
					<Home class="w-7 h-7" />
					<span class="hidden xl:inline">Home</span>
				</Button>
			</a>

			<a href="/explore">
				<Button
					variant="ghost"
					class={cn(
						'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
						isActive('/explore') && 'font-bold'
					)}
				>
					<Compass class="w-7 h-7" />
					<span class="hidden xl:inline">Explore</span>
				</Button>
			</a>

			{#if $currentUser}
				<a href="/notifications">
					<Button
						variant="ghost"
						class={cn(
							'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
							isActive('/notifications') && 'font-bold'
						)}
					>
						<Bell class="w-7 h-7" />
						<span class="hidden xl:inline">Notifications</span>
					</Button>
				</a>

				<a href="/messages">
					<Button
						variant="ghost"
						class={cn(
							'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
							isActive('/messages') && 'font-bold'
						)}
					>
						<Mail class="w-7 h-7" />
						<span class="hidden xl:inline">Messages</span>
					</Button>
				</a>

				<a href="/dvm">
					<Button
						variant="ghost"
						class={cn(
							'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
							isActive('/dvm') && 'font-bold'
						)}
					>
						<Zap class="w-7 h-7" />
						<span class="hidden xl:inline">DVM</span>
					</Button>
				</a>

				<a href="/lists">
					<Button
						variant="ghost"
						class={cn(
							'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
							isActive('/lists') && 'font-bold'
						)}
					>
						<List class="w-7 h-7" />
						<span class="hidden xl:inline">Lists</span>
					</Button>
				</a>

				<a href="/bookmarks">
					<Button
						variant="ghost"
						class={cn(
							'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
							isActive('/bookmarks') && 'font-bold'
						)}
					>
						<Bookmark class="w-7 h-7" />
						<span class="hidden xl:inline">Bookmarks</span>
					</Button>
				</a>

				<a href="/communities">
					<Button
						variant="ghost"
						class={cn(
							'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
							isActive('/communities') && 'font-bold'
						)}
					>
						<Users class="w-7 h-7" />
						<span class="hidden xl:inline">Communities</span>
					</Button>
				</a>

				<a href={profilePath}>
					<Button
						variant="ghost"
						class={cn(
							'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
							isActive(profilePath) && 'font-bold'
						)}
					>
						<User class="w-7 h-7" />
						<span class="hidden xl:inline">Profile</span>
					</Button>
				</a>

				<a href="/settings">
					<Button
						variant="ghost"
						class={cn(
							'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent',
							isActive('/settings') && 'font-bold'
						)}
					>
						<Settings class="w-7 h-7" />
						<span class="hidden xl:inline">Settings</span>
					</Button>
				</a>
			{/if}

			<!-- More menu -->
			{#if mounted}
				<Popover.Root bind:open={moreOpen}>
					<Popover.Trigger>
						<Button
							variant="ghost"
							class={cn(
								'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent'
							)}
						>
							<MoreHorizontal class="w-7 h-7" />
							<span class="hidden xl:inline">More</span>
						</Button>
					</Popover.Trigger>
					<Popover.Content class="w-80 p-0" align="start" side="top">
						<div class="flex flex-col">
							<a
								href="https://vlogstr.com"
								target="_blank"
								rel="noopener noreferrer"
								onclick={() => (moreOpen = false)}
							>
								<Button
									variant="ghost"
									class="w-full justify-start gap-4 text-base py-4 px-4 rounded-none hover:bg-accent"
								>
									<Video class="w-5 h-5" />
									<span>Vlogstr</span>
								</Button>
							</a>
							<a
								href="https://nostrcal.com"
								target="_blank"
								rel="noopener noreferrer"
								onclick={() => (moreOpen = false)}
							>
								<Button
									variant="ghost"
									class="w-full justify-start gap-4 text-base py-4 px-4 rounded-none hover:bg-accent"
								>
									<Calendar class="w-5 h-5" />
									<span>nostrcal</span>
								</Button>
							</a>
							<a
								href="https://nostrmusic.com"
								target="_blank"
								rel="noopener noreferrer"
								onclick={() => (moreOpen = false)}
							>
								<Button
									variant="ghost"
									class="w-full justify-start gap-4 text-base py-4 px-4 rounded-none hover:bg-accent"
								>
									<Music class="w-5 h-5" />
									<span>nostrmusic</span>
								</Button>
							</a>
						</div>
					</Popover.Content>
				</Popover.Root>
			{:else}
				<!-- Fallback button while mounting -->
				<Button
					variant="ghost"
					class={cn(
						'w-full justify-start gap-4 text-xl py-6 px-4 rounded-full hover:bg-accent'
					)}
				>
					<MoreHorizontal class="w-7 h-7" />
					<span class="hidden xl:inline">More</span>
				</Button>
			{/if}
		</nav>

		<!-- Post Button -->
		{#if $currentUser}
			<div class="mt-4 px-2">
				<Button
					onclick={() => (composeOpen = true)}
					class="w-full rounded-full py-6 text-lg font-bold"
					size="lg"
				>
					<PenSquare class="w-6 h-6 xl:mr-2" />
					<span class="hidden xl:inline">Post</span>
				</Button>
			</div>
		{/if}
	</div>

	<!-- Login/Account Section -->
	<div class="mt-auto px-2 pb-4">
		<LoginArea />
	</div>
</div>

<!-- Compose Dialog -->
<Dialog.Root bind:open={composeOpen}>
	<Dialog.Content class="max-w-2xl">
		<NoteComposer bind:isOpen={composeOpen} onClose={() => (composeOpen = false)} />
	</Dialog.Content>
</Dialog.Root>
