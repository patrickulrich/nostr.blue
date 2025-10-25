<script lang="ts">
	import type { TrustedEvent } from '@welshman/util';
	import { createQuery } from '@tanstack/svelte-query';
	import { loadWithRouter } from '$lib/services/outbox';
	import { nip19 } from 'nostr-tools';
	import { Avatar, AvatarFallback, AvatarImage } from '$lib/components/ui/avatar';
	import { Button } from '$lib/components/ui/button';
	import NoteContent from './NoteContent.svelte';
	import ReactionButton from './ReactionButton.svelte';
	import RepostButton from './RepostButton.svelte';
	import ZapButton from './ZapButton.svelte';
	import BookmarkButton from './BookmarkButton.svelte';
	import { genUserName } from '$lib/genUserName';
	import { useReplyCount } from '$lib/hooks/useReplyCount.svelte';
	import { formatDistanceToNow } from '$lib/utils/formatTime';
	import { cn } from '$lib/utils';
	import { MessageCircle, MoreHorizontal, Share } from 'lucide-svelte';
	import { goto } from '$app/navigation';

	interface Props {
		event: TrustedEvent;
		showThread?: boolean;
		class?: string;
	}

	let { event, showThread = true, class: className = '' }: Props = $props();

	interface AuthorMetadata {
		display_name?: string;
		name?: string;
		picture?: string;
	}

	// Fetch author profile
	const authorQuery = createQuery<AuthorMetadata>(() => ({
		queryKey: ['profile', event.pubkey],
		queryFn: async () => {
			const profiles = await loadWithRouter({
				filters: [
					{
						kinds: [0],
						authors: [event.pubkey],
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
		staleTime: 5 * 60 * 1000
	}));

	const replyCountQuery = useReplyCount(event.id);

	let npub = $derived(nip19.npubEncode(event.pubkey));
	let noteId = $derived(nip19.noteEncode(event.id));

	let displayName = $derived(
		authorQuery.data?.display_name || authorQuery.data?.name || genUserName(event.pubkey)
	);

	let username = $derived(authorQuery.data?.name || `@${npub.slice(0, 12)}...`);
	let profilePicture = $derived(authorQuery.data?.picture);
	let timestamp = $derived(formatDistanceToNow(event.created_at));
	let replyCount = $derived(replyCountQuery.data || 0);

	function handleCardClick(e: MouseEvent) {
		// Don't navigate if clicking on a button, link, or interactive element
		const target = e.target as HTMLElement;
		if (target.closest('button') || target.closest('a') || target.closest('[role="button"]')) {
			return;
		}
		goto(`/${noteId}`);
	}

	function handleShare(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();

		if (navigator.share) {
			navigator.share({
				title: `Post by ${displayName}`,
				url: `${window.location.origin}/${noteId}`
			});
		} else {
			// Fallback: copy to clipboard
			navigator.clipboard.writeText(`${window.location.origin}/${noteId}`);
		}
	}
</script>

<div
	class={cn(
		'border-b border-border hover:bg-accent/50 transition-colors cursor-pointer',
		className
	)}
	onclick={handleCardClick}
	role="button"
	tabindex="0"
	onkeydown={(e) => e.key === 'Enter' && handleCardClick(e as any)}
>
	<div class="flex gap-3 p-4">
		<!-- Avatar -->
		<a href="/{npub}" class="flex-shrink-0" onclick={(e) => e.stopPropagation()}>
			<Avatar class="w-12 h-12">
				<AvatarImage src={profilePicture} alt={displayName} />
				<AvatarFallback>{displayName[0]?.toUpperCase() || 'A'}</AvatarFallback>
			</Avatar>
		</a>

		<!-- Content -->
		<div class="flex-1 min-w-0">
			<!-- Header -->
			<div class="flex items-start justify-between gap-2 mb-1">
				<div class="flex items-center gap-2 flex-wrap min-w-0">
					<a
						href="/{npub}"
						class="font-bold hover:underline truncate"
						onclick={(e) => e.stopPropagation()}
					>
						{displayName}
					</a>
					<a
						href="/{npub}"
						class="text-muted-foreground text-sm truncate"
						onclick={(e) => e.stopPropagation()}
					>
						{username}
					</a>
					<span class="text-muted-foreground text-sm">·</span>
					<a
						href="/{noteId}"
						class="text-muted-foreground text-sm hover:underline"
						onclick={(e) => e.stopPropagation()}
					>
						{timestamp}
					</a>
				</div>
				<Button variant="ghost" size="icon" class="flex-shrink-0 -mt-1 -mr-2">
					<MoreHorizontal class="h-4 w-4" />
				</Button>
			</div>

			<!-- Post Content -->
			<div class="block">
				<NoteContent {event} class="text-base mb-3" />
			</div>

			<!-- Actions -->
			<div class="flex items-center justify-between max-w-lg mt-2">
				<Button
					variant="ghost"
					size="sm"
					class="text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 gap-1 -ml-2"
					onclick={(e) => {
						e.preventDefault();
						e.stopPropagation();
						// TODO: Open reply dialog
					}}
				>
					<MessageCircle class="h-[18px] w-[18px]" />
					<span class="text-xs">
						{replyCount > 500 ? '500+' : replyCount > 0 ? replyCount : ''}
					</span>
				</Button>

				<RepostButton {event} />

				<ReactionButton eventId={event.id} authorPubkey={event.pubkey} />

				<ZapButton target={event} showCount={true} />

				<div class="flex items-center gap-0">
					<BookmarkButton eventId={event.id} />
					<Button
						variant="ghost"
						size="sm"
						class="text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 p-2"
						onclick={handleShare}
					>
						<Share class="h-[18px] w-[18px]" />
					</Button>
				</div>
			</div>
		</div>
	</div>
</div>
