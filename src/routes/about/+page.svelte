<script lang="ts">
	import MainLayout from '$lib/components/MainLayout.svelte';
	import AppSidebar from '$lib/components/AppSidebar.svelte';
	import RightSidebar from '$lib/components/RightSidebar.svelte';
	import { Github, Globe, Zap } from 'lucide-svelte';
</script>

<svelte:head>
	<title>About - nostr.blue</title>
	<meta name="description" content="About nostr.blue - A decentralized social network on Nostr" />
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
			<h1 class="text-4xl font-bold mb-8">About nostr.blue</h1>

			<div class="prose dark:prose-invert max-w-none space-y-8">
				<section class="space-y-4">
					<h2 class="text-2xl font-semibold">What is nostr.blue?</h2>
					<p>
						nostr.blue is a modern social network client built on the Nostr protocol - a decentralized,
						censorship-resistant social network. Unlike traditional social media platforms, Nostr gives you
						true ownership of your identity and content.
					</p>
					<p>
						With nostr.blue, you can connect with people, share ideas, and participate in communities
						without relying on centralized corporations that control your data and censor your speech.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold">Key Features</h2>
					<div class="grid md:grid-cols-2 gap-4 mt-4">
						<div class="border border-border rounded-lg p-4">
							<h3 class="font-semibold mb-2">🔐 True Ownership</h3>
							<p class="text-sm text-muted-foreground">
								Your identity is controlled by cryptographic keys that only you possess.
								No company can ban, shadowban, or censor you.
							</p>
						</div>

						<div class="border border-border rounded-lg p-4">
							<h3 class="font-semibold mb-2">⚡ Lightning Zaps</h3>
							<p class="text-sm text-muted-foreground">
								Send and receive Bitcoin payments directly on posts using the Lightning Network.
								Support creators instantly with micropayments.
							</p>
						</div>

						<div class="border border-border rounded-lg p-4">
							<h3 class="font-semibold mb-2">🤖 Data Vending Machines</h3>
							<p class="text-sm text-muted-foreground">
								Access AI-powered services that curate content, provide recommendations,
								and process data - all on Nostr.
							</p>
						</div>

						<div class="border border-border rounded-lg p-4">
							<h3 class="font-semibold mb-2">👥 Communities</h3>
							<p class="text-sm text-muted-foreground">
								Join NIP-72 moderated communities for topic-specific discussions and collaborations.
							</p>
						</div>

						<div class="border border-border rounded-lg p-4">
							<h3 class="font-semibold mb-2">🔒 Private Messaging</h3>
							<p class="text-sm text-muted-foreground">
								End-to-end encrypted direct messages that only you and your recipient can read.
							</p>
						</div>

						<div class="border border-border rounded-lg p-4">
							<h3 class="font-semibold mb-2">📋 Lists & Bookmarks</h3>
							<p class="text-sm text-muted-foreground">
								Organize your Nostr experience with custom lists, bookmarks, and curated feeds.
							</p>
						</div>
					</div>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold">Technology</h2>
					<p>
						nostr.blue is built with modern web technologies for optimal performance and user experience:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li><strong>Svelte 5:</strong> Modern reactive framework for fast, efficient UIs</li>
						<li><strong>SvelteKit:</strong> Full-stack framework with server-side rendering</li>
						<li><strong>Welshman:</strong> Battle-tested Nostr toolkit from Coracle</li>
						<li><strong>TanStack Query:</strong> Powerful data fetching and caching</li>
						<li><strong>TypeScript:</strong> Type-safe development</li>
					</ul>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold">Open Source</h2>
					<p>
						nostr.blue is open source software. You can view, audit, and contribute to the code on GitHub.
						We believe in transparency and community-driven development.
					</p>
					<div class="flex gap-4 mt-4">
						<a
							href="https://github.com/patrickulrich/nostr.blue"
							target="_blank"
							rel="noopener noreferrer"
							class="inline-flex items-center gap-2 px-4 py-2 bg-primary text-primary-foreground rounded-lg hover:bg-primary/90 transition-colors no-underline"
						>
							<Github class="h-5 w-5" />
							View on GitHub
						</a>
					</div>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold">Nostr Protocol</h2>
					<p>
						Nostr (Notes and Other Stuff Transmitted by Relays) is a simple, open protocol that enables
						global, decentralized, and censorship-resistant social media. Learn more:
					</p>
					<div class="flex gap-4 mt-4">
						<a
							href="https://nostr.com"
							target="_blank"
							rel="noopener noreferrer"
							class="inline-flex items-center gap-2 px-4 py-2 border border-border rounded-lg hover:bg-accent transition-colors no-underline"
						>
							<Globe class="h-5 w-5" />
							nostr.com
						</a>
					</div>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold">Privacy & Security</h2>
					<p>
						Your privacy is our priority. nostr.blue operates as a client-side application -
						your private keys never leave your device, and we don't collect or store your personal data.
					</p>
					<p>
						Read our <a href="/privacy" class="text-blue-500 hover:underline">Privacy Policy</a> and
						<a href="/cookies" class="text-blue-500 hover:underline">Cookie Policy</a> to learn more.
					</p>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold">Support the Project</h2>
					<p>
						nostr.blue is free and open source. If you'd like to support development, you can:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li>Contribute code on GitHub</li>
						<li>Report bugs and suggest features</li>
						<li>Share nostr.blue with others</li>
						<li>Send Lightning zaps to the developers on Nostr</li>
					</ul>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold">Legal</h2>
					<p>
						By using nostr.blue, you agree to our:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li><a href="/terms" class="text-blue-500 hover:underline">Terms of Service</a></li>
						<li><a href="/privacy" class="text-blue-500 hover:underline">Privacy Policy</a></li>
						<li><a href="/cookies" class="text-blue-500 hover:underline">Cookie Policy</a></li>
					</ul>
				</section>

				<section class="space-y-4">
					<h2 class="text-2xl font-semibold">Acknowledgments</h2>
					<p>
						nostr.blue is built on the shoulders of giants. Special thanks to:
					</p>
					<ul class="list-disc pl-6 space-y-2">
						<li><a href="https://coracle.social/" target="_blank" rel="noopener noreferrer" class="text-blue-500 hover:underline">Coracle</a> - For pioneering Welshman and Nostr client patterns</li>
						<li><a href="https://github.com/coracle-social/welshman" target="_blank" rel="noopener noreferrer" class="text-blue-500 hover:underline">Welshman</a> - Battle-tested Nostr toolkit</li>
						<li><a href="https://shadcn-svelte.com/" target="_blank" rel="noopener noreferrer" class="text-blue-500 hover:underline">shadcn-svelte</a> - Beautiful accessible components</li>
						<li>The Nostr Community - For building the decentralized web</li>
					</ul>
				</section>

				<footer class="text-center text-sm text-muted-foreground mt-12 pt-8 border-t border-border">
					<p>
						Built with ⚡ on Nostr | <a href="https://github.com/patrickulrich/nostr.blue" class="text-blue-500 hover:underline">Open Source</a>
					</p>
				</footer>
			</div>
		</div>
	{/snippet}
</MainLayout>
