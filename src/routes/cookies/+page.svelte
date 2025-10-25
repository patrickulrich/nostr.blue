<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
</script>

<svelte:head>
	<title>Cookie Policy - nostr.blue</title>
	<meta name="description" content="Cookie Policy for nostr.blue" />
</svelte:head>

<MainLayout>
	{#snippet sidebar()}
		<AppSidebar />
	{/snippet}

	{#snippet rightPanel()}
		<RightSidebar />
	{/snippet}

	{#snippet children()}
		<div class="max-w-4xl mx-auto px-6 py-12">
			<h1 class="text-4xl font-bold mb-8">Cookie Policy</h1>

			<div class="prose dark:prose-invert max-w-none space-y-6">
				<p class="text-lg text-muted-foreground">Last updated: {new Date().toLocaleDateString()}</p>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">1. What Are Cookies?</h2>
					<p>
						Cookies are small text files that are stored on your device when you visit a website.
						They help websites remember your preferences and provide functionality.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">2. How nostr.blue Uses Cookies</h2>
					<p>
						nostr.blue uses minimal cookies and local storage for essential functionality only.
						We do NOT use cookies for:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li>Advertising or marketing</li>
						<li>User tracking across websites</li>
						<li>Analytics or behavioral profiling</li>
						<li>Third-party data sharing</li>
					</ul>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">3. Essential Cookies and Local Storage</h2>
					<p>
						We use browser local storage (not cookies) for:
					</p>

					<div class="space-y-4 mt-4">
						<div class="border border-border rounded-lg p-4">
							<h3 class="font-semibold mb-2">Authentication</h3>
							<p class="text-sm text-muted-foreground">
								Stores your Nostr private keys or session information (if you log in with nsec).
								This data is encrypted and never leaves your device.
							</p>
							<p class="text-xs mt-2"><strong>Storage Type:</strong> LocalStorage</p>
							<p class="text-xs"><strong>Duration:</strong> Until you log out or clear browser data</p>
						</div>

						<div class="border border-border rounded-lg p-4">
							<h3 class="font-semibold mb-2">User Preferences</h3>
							<p class="text-sm text-muted-foreground">
								Remembers your theme preference (light/dark mode), default relays, and other settings.
							</p>
							<p class="text-xs mt-2"><strong>Storage Type:</strong> LocalStorage</p>
							<p class="text-xs"><strong>Duration:</strong> Persistent until cleared</p>
						</div>

						<div class="border border-border rounded-lg p-4">
							<h3 class="font-semibold mb-2">Performance Cache</h3>
							<p class="text-sm text-muted-foreground">
								Temporarily caches Nostr events and profile data to improve loading times.
							</p>
							<p class="text-xs mt-2"><strong>Storage Type:</strong> IndexedDB / Cache API</p>
							<p class="text-xs"><strong>Duration:</strong> Varies, automatically cleared when old</p>
						</div>
					</div>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">4. No Third-Party Cookies</h2>
					<p>
						nostr.blue does not use any third-party cookies for analytics, advertising, or tracking.
						We respect your privacy and do not share your data with advertisers or data brokers.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">5. Managing Cookies and Local Storage</h2>
					<p>
						You can manage or clear cookies and local storage through your browser settings:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li><strong>Chrome:</strong> Settings → Privacy and security → Cookies and other site data</li>
						<li><strong>Firefox:</strong> Settings → Privacy & Security → Cookies and Site Data</li>
						<li><strong>Safari:</strong> Preferences → Privacy → Manage Website Data</li>
						<li><strong>Edge:</strong> Settings → Cookies and site permissions → Cookies and site data</li>
					</ul>
					<p class="mt-4">
						<strong>Note:</strong> Clearing local storage will log you out and reset your preferences.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">6. Browser Extensions</h2>
					<p>
						If you use Nostr browser extensions (like Alby or nos2x) for authentication,
						those extensions may set their own cookies or storage. Please review their privacy policies.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">7. Changes to Cookie Policy</h2>
					<p>
						We may update this policy to reflect changes in technology or legal requirements.
						Continued use of nostr.blue constitutes acceptance of any updates.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">8. Contact</h2>
					<p>
						For questions about cookies or data storage, visit our <a href="/about" class="text-blue-500 hover:underline">About</a> page.
					</p>
				</section>
			</div>
		</div>
	{/snippet}
</MainLayout>
