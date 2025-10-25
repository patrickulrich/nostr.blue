<script lang="ts">
	import type { TrustedEvent } from '@welshman/util';
	import { parse, reduceLinks, isImage, isLink, isProfile, isEvent, isTopic } from '@welshman/content';
	import type { Parsed } from '@welshman/content';
	import { createQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import { nip19 } from 'nostr-tools';
	import { cn } from '$lib/utils';
	import { genUserName } from '$lib/genUserName';
	import NoteContent from './NoteContent.svelte';

	interface Props {
		event: TrustedEvent;
		class?: string;
		depth?: number;
	}

	let { event, class: className, depth = 0 }: Props = $props();

	// Parse content using @welshman/content
	let parsedContent = $derived.by(() => {
		const parsed = parse({ content: event.content, tags: event.tags });
		return reduceLinks(parsed);
	});

	// Extract event references for fetching
	let eventReferences = $derived.by(() => {
		const refs: string[] = [];
		for (const item of parsedContent) {
			if (isEvent(item) && depth === 0) {
				refs.push(item.value.id);
			}
		}
		return refs;
	});

	// Fetch referenced events
	const referencedEventsQuery = createQuery(() => ({
		queryKey: ['referenced-notes', eventReferences, depth],
		queryFn: async ({ signal }) => {
			if (eventReferences.length === 0 || depth > 0) return {};

			const events = await loadWithRouter({
				filters: [{ ids: eventReferences }],
				signal,
			});

			const eventMap: Record<string, TrustedEvent> = {};
			events.forEach((evt) => {
				eventMap[evt.id] = evt;
			});

			return eventMap;
		},
		enabled: eventReferences.length > 0 && depth === 0,
		staleTime: 60000
	}));

	let referencedEvents = $derived(referencedEventsQuery.data || {});

	// Extract media URLs using welshman's parser (like Coracle does)
	// Parse content WITHOUT reduceLinks to get raw image links
	let rawContent = $derived(parse({ content: event.content, tags: event.tags }));

	// Helper to check if a URL is media (image or video)
	function isMediaUrl(url: string): boolean {
		return /\.(jpe?g|png|gif|webp|mp4|webm|ogg|mov)(\?.*)?$/i.test(url);
	}

	function isVideoUrl(url: string): boolean {
		return /\.(mp4|webm|ogg|mov)(\?.*)?$/i.test(url);
	}

	// Helper to check if a URL is a YouTube link (including mobile subdomain)
	function isYouTubeUrl(url: string): boolean {
		return /^(https?:\/\/)?(www\.|m\.)?(youtube\.com|youtu\.be)\/.+$/i.test(url);
	}

	// Helper to extract YouTube video ID from URL
	function getYouTubeVideoId(url: string): string | null {
		// Handle youtube.com/watch?v=VIDEO_ID
		const watchMatch = url.match(/[?&]v=([^&]+)/);
		if (watchMatch) return watchMatch[1];

		// Handle youtu.be/VIDEO_ID
		const shortMatch = url.match(/youtu\.be\/([^?]+)/);
		if (shortMatch) return shortMatch[1];

		// Handle youtube.com/embed/VIDEO_ID
		const embedMatch = url.match(/youtube\.com\/embed\/([^?]+)/);
		if (embedMatch) return embedMatch[1];

		return null;
	}

	// Extract media URLs from raw parsed content (before reduceLinks)
	// Get ALL links that look like media (images AND videos)
	let mediaUrls = $derived.by(() => {
		return rawContent
			.filter(isLink)
			.map((p) => p.value.url?.toString())
			.filter((url): url is string => typeof url === 'string' && isMediaUrl(url));
	});

	// Extract YouTube URLs from raw parsed content
	let youtubeUrls = $derived.by(() => {
		return rawContent
			.filter(isLink)
			.map((p) => {
				// Handle different value structures
				if (typeof p.value === 'string') {
					return p.value;
				} else if (p.value && typeof p.value === 'object' && 'url' in p.value) {
					return p.value.url?.toString();
				}
				return null;
			})
			.filter((url): url is string => typeof url === 'string' && isYouTubeUrl(url));
	});
</script>

<div class={cn('break-words', className)}>
	<!-- Text content -->
	<div class="whitespace-pre-wrap">
		{#each parsedContent as item}
			{#if item.type === 'text'}
				{item.value}
			{:else if item.type === 'newline'}
				<br />
			{:else if isLink(item) && !isImage(item)}
				{#if typeof (item as any).value === 'string'}
					{@const linkValue = (item as any).value as string}
					{#if !isMediaUrl(linkValue) && !isYouTubeUrl(linkValue)}
						<a
							href={linkValue}
							target="_blank"
							rel="noopener noreferrer"
							class="text-blue-500 hover:underline break-all"
						>
							{linkValue}
						</a>
					{/if}
				{/if}
			{:else if item.type === 'link-grid' && Array.isArray(item.value)}
				<!-- Link Grid - multiple adjacent links -->
				<div class="flex flex-wrap gap-2 my-2">
					{#each item.value as link}
						{#if isLink(link) && typeof link.value === 'string' && !isMediaUrl(link.value) && !isYouTubeUrl(link.value)}
							<a
								href={link.value}
								target="_blank"
								rel="noopener noreferrer"
								class="text-sm text-blue-500 hover:underline break-all"
							>
								{link.value}
							</a>
						{/if}
					{/each}
				</div>
			{:else if isProfile(item)}
				{@const pubkey = item.value.pubkey}
				{@const npub = nip19.npubEncode(pubkey)}
				{@const authorQuery = createQuery(() => ({
					queryKey: ['profile', pubkey],
					queryFn: async () => {
						const profiles = await loadWithRouter({
							filters: [{ kinds: [0], authors: [pubkey], limit: 1 }]
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
					staleTime: 5 * 60 * 1000
				}))}
				{@const displayName = authorQuery.data?.name || genUserName(pubkey)}
				<a
					href="/{npub}"
					class="font-semibold text-blue-500 hover:underline"
					onclick={(e) => e.stopPropagation()}
				>
					@{displayName}
				</a>
			{:else if isEvent(item) && depth === 0}
				{@const eventId = item.value.id}
				{@const embeddedEvent = referencedEvents[eventId]}

				{#if embeddedEvent}
					<!-- Embedded note card -->
					{@const noteId = nip19.noteEncode(embeddedEvent.id)}
					{@const npub = nip19.npubEncode(embeddedEvent.pubkey)}
					{@const authorQuery = createQuery(() => ({
						queryKey: ['profile', embeddedEvent.pubkey],
						queryFn: async () => {
							const profiles = await loadWithRouter({
								filters: [{ kinds: [0], authors: [embeddedEvent.pubkey], limit: 1 }]
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
						staleTime: 5 * 60 * 1000
					}))}
					{@const displayName = authorQuery.data?.name || genUserName(embeddedEvent.pubkey)}
					{@const username = authorQuery.data?.name || npub.slice(0, 12)}
					{@const avatarUrl = authorQuery.data?.picture}

					<div class="block border border-border rounded-xl p-3 hover:bg-accent/50 transition-colors my-3">
						<a
							href="/{noteId}"
							class="flex items-start gap-2 mb-2 hover:underline"
							onclick={(e) => e.stopPropagation()}
						>
							{#if avatarUrl}
								<img src={avatarUrl} alt={displayName} class="w-5 h-5 rounded-full" />
							{:else}
								<div
									class="w-5 h-5 rounded-full bg-muted flex items-center justify-center text-xs"
								>
									{displayName[0]?.toUpperCase()}
								</div>
							{/if}
							<div class="flex flex-col min-w-0">
								<span class="font-semibold text-sm truncate">{displayName}</span>
								<span class="text-xs text-muted-foreground truncate">@{username}</span>
							</div>
						</a>
						<NoteContent event={embeddedEvent} class="text-sm" depth={1} />
					</div>
				{:else}
					<!-- Event not loaded, show as link -->
					{@const nip19Id = item.value.relays?.length
						? nip19.neventEncode({ id: eventId, relays: item.value.relays })
						: nip19.noteEncode(eventId)}
					<a href="/{nip19Id}" class="text-blue-500 hover:underline">
						nostr:{nip19Id}
					</a>
				{/if}
			{:else if isEvent(item)}
				<!-- Depth > 0, just show link -->
				{@const eventId = item.value.id}
				{@const nip19Id = nip19.noteEncode(eventId)}
				<a href="/{nip19Id}" class="text-blue-500 hover:underline">
					nostr:{nip19Id}
				</a>
			{:else if isTopic(item)}
				<a href="/t/{item.value}" class="text-blue-500 hover:underline">
					#{item.value}
				</a>
			{:else if item.type === 'code'}
				<code class="px-1.5 py-0.5 rounded bg-muted font-mono text-sm">
					{item.value}
				</code>
			{:else if item.type === 'emoji' && typeof item.value === 'string'}
				<!-- Custom emoji support -->
				{@const emojiName = item.value as string}
				{@const emojiTag = event.tags.find(
					(tag) => tag[0] === 'emoji' && tag[1] === emojiName
				)}
				{#if emojiTag && emojiTag[2]}
					<img
						src={emojiTag[2]}
						alt=":{emojiName}:"
						class="inline h-5 w-5 align-middle"
						title=":{emojiName}:"
					/>
				{:else}
					:{emojiName}:
				{/if}
			{:else if item.type === 'cashu'}
				<span
					class="inline-block px-2 py-1 rounded bg-green-500/10 text-green-600 dark:text-green-400 font-mono text-sm"
					title="Cashu token"
				>
					{item.value.slice(0, 20)}...
				</span>
			{:else if item.type === 'invoice'}
				<a
					href="lightning:{item.value}"
					class="inline-block px-2 py-1 rounded bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 font-mono text-sm hover:underline"
					title="Lightning invoice"
				>
					{item.value.slice(0, 20)}...⚡
				</a>
			{/if}
		{/each}
	</div>

	<!-- Media attachments -->
	{#if mediaUrls.length > 0}
		<div class="mt-2 space-y-2">
			{#each mediaUrls as mediaUrl}
				{#if isVideoUrl(mediaUrl)}
					<div class="my-2 rounded-lg overflow-hidden border border-border max-w-full">
						<video src={mediaUrl} controls class="max-w-full h-auto max-h-[500px]">
							<track kind="captions" />
						</video>
					</div>
				{:else}
					<button
						type="button"
						class="my-2 rounded-lg overflow-hidden border border-border max-w-full block cursor-pointer hover:opacity-90 transition-opacity p-0 bg-transparent"
						onclick={() => window.open(mediaUrl, '_blank')}
					>
						<img
							src={mediaUrl}
							alt=""
							class="max-w-full h-auto max-h-[500px] object-contain"
						/>
					</button>
				{/if}
			{/each}
		</div>
	{/if}

	<!-- YouTube embeds -->
	{#if youtubeUrls.length > 0}
		<div class="mt-2 space-y-2">
			{#each youtubeUrls as youtubeUrl}
				{@const videoId = getYouTubeVideoId(youtubeUrl)}
				{#if videoId}
					<div class="my-2 rounded-lg overflow-hidden border border-border max-w-full aspect-video">
						<iframe
							src="https://www.youtube.com/embed/{videoId}"
							title="YouTube video"
							allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
							allowfullscreen
							loading="lazy"
							class="w-full h-full border-0"
						></iframe>
					</div>
				{/if}
			{/each}
		</div>
	{/if}
</div>
