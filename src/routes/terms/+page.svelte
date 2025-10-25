<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
</script>

<svelte:head>
	<title>Terms of Service - nostr.blue</title>
	<meta name="description" content="Terms of Service for nostr.blue" />
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
			<h1 class="text-4xl font-bold mb-8">Terms of Service</h1>

			<div class="prose dark:prose-invert max-w-none space-y-6">
				<p class="text-lg text-muted-foreground">Last updated: {new Date().toLocaleDateString()}</p>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">1. Acceptance of Terms</h2>
					<p>
						By accessing and using nostr.blue, you accept and agree to be bound by the terms and provisions of this agreement.
						nostr.blue is a decentralized social network client built on the Nostr protocol.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">2. Description of Service</h2>
					<p>
						nostr.blue provides a user interface for accessing the Nostr protocol, a decentralized social network.
						The service allows users to publish, read, and interact with content on the Nostr network.
					</p>
					<p>
						As a client for a decentralized protocol, nostr.blue does not host, store, or control the content published
						through the service. All content is distributed across the Nostr network via independent relay servers.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">3. User Responsibilities</h2>
					<p>Users are responsible for:</p>
					<ul class="list-disc pl-6 space-y-2">
						<li>Maintaining the security of their private keys and account credentials</li>
						<li>All content they publish through the service</li>
						<li>Complying with applicable laws and regulations</li>
						<li>Respecting the intellectual property rights of others</li>
					</ul>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">4. Privacy and Data</h2>
					<p>
						nostr.blue operates as a client-side application. Your private keys and sensitive data are stored locally
						in your browser and are never transmitted to our servers. See our <a href="/privacy" class="text-blue-500 hover:underline">Privacy Policy</a> for more details.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">5. Content and Conduct</h2>
					<p>
						While nostr.blue does not control content on the Nostr network, users agree not to use the service to publish:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li>Illegal content</li>
						<li>Content that infringes on intellectual property rights</li>
						<li>Malicious software or code</li>
						<li>Spam or unsolicited commercial content</li>
					</ul>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">6. Disclaimer of Warranties</h2>
					<p>
						nostr.blue is provided "as is" without warranties of any kind, either express or implied.
						We do not guarantee the availability, accuracy, or reliability of the service.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">7. Limitation of Liability</h2>
					<p>
						nostr.blue and its operators shall not be liable for any indirect, incidental, special, consequential,
						or punitive damages resulting from your use of the service.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">8. Changes to Terms</h2>
					<p>
						We reserve the right to modify these terms at any time. Continued use of the service following
						any changes constitutes acceptance of those changes.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold mt-8">9. Contact</h2>
					<p>
						For questions about these Terms of Service, please visit our <a href="/about" class="text-blue-500 hover:underline">About</a> page.
					</p>
				</section>
			</div>
		</div>
	{/snippet}
</MainLayout>
