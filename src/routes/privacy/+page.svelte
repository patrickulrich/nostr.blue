<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
</script>

<svelte:head>
	<title>Privacy Policy - nostr.blue</title>
	<meta name="description" content="Privacy Policy for nostr.blue" />
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
			<h1 class="text-4xl font-bold mb-8">Privacy Policy</h1>

			<div class="prose dark:prose-invert max-w-none space-y-6">
				<p class="text-lg text-muted-foreground">Last updated: {new Date().toLocaleDateString()}</p>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">1. Overview</h2>
					<p>
						nostr.blue is committed to protecting your privacy. As a client for the decentralized Nostr protocol,
						we have designed our service to minimize data collection and maximize user control over their information.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">2. Data We Don't Collect</h2>
					<p>
						nostr.blue operates as a client-side application. We do NOT collect, store, or have access to:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li>Your private keys (nsec) - these remain in your browser's local storage</li>
						<li>Your personal messages or direct messages</li>
						<li>Your browsing history within the application</li>
						<li>Your IP address for tracking purposes</li>
						<li>Personal information beyond what you choose to publish on Nostr</li>
					</ul>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">3. Local Data Storage</h2>
					<p>
						The following data is stored locally in your browser:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li>Your Nostr private keys (if you choose to log in with nsec)</li>
						<li>Application preferences (theme, default relays, etc.)</li>
						<li>Cached content for performance</li>
					</ul>
					<p>
						This data never leaves your device unless you explicitly publish it to the Nostr network.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">4. Public Data on Nostr</h2>
					<p>
						When you publish content through nostr.blue:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li>Posts, reactions, and public interactions are broadcast to Nostr relay servers</li>
						<li>This content becomes publicly accessible on the Nostr network</li>
						<li>Your public key (npub) is associated with your content</li>
						<li>Content may be cached and distributed across multiple relays</li>
					</ul>
					<p>
						Remember: Anything you post publicly on Nostr is permanent and decentralized.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">5. Third-Party Services</h2>
					<p>
						nostr.blue connects to:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li><strong>Nostr Relays</strong>: Independent servers that relay Nostr events</li>
						<li><strong>Lightning Network Services</strong>: For zap payments (if you use this feature)</li>
						<li><strong>Media Hosting</strong>: When you view images/videos from external URLs</li>
					</ul>
					<p>
						These services have their own privacy policies. We recommend reviewing them.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">6. Cookies and Tracking</h2>
					<p>
						nostr.blue uses minimal cookies for essential functionality only. See our
						<a href="/cookies" class="text-blue-500 hover:underline">Cookie Policy</a> for details.
						We do not use analytics, advertising, or tracking cookies.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">7. Security</h2>
					<p>
						We implement industry-standard security practices:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li>HTTPS encryption for all connections</li>
						<li>Client-side cryptography for signing events</li>
						<li>Support for hardware signers and browser extensions</li>
						<li>Regular security audits of our code</li>
					</ul>
					<p>
						However, you are responsible for keeping your private keys secure.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">8. Your Rights</h2>
					<p>
						You have the right to:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li>Delete your local data by clearing browser storage</li>
						<li>Export your private keys and use them with other Nostr clients</li>
						<li>Request deletion from specific relays (though we cannot guarantee removal)</li>
					</ul>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">9. Children's Privacy</h2>
					<p>
						nostr.blue is not intended for users under 13 years of age. We do not knowingly collect
						information from children.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">10. Changes to Privacy Policy</h2>
					<p>
						We may update this policy from time to time. Continued use of the service after changes
						constitutes acceptance of the updated policy.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">11. Contact</h2>
					<p>
						For privacy questions, please visit our <a href="/about" class="text-blue-500 hover:underline">About</a> page.
					</p>
				</section>
			</div>
		</div>
	{/snippet}
</MainLayout>
