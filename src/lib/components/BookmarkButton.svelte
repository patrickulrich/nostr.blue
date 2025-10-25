<script lang="ts">
	import { Bookmark } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import { useBookmarks } from '$lib/hooks/useBookmarks.svelte';
	import { useToast } from '$lib/stores/toast.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { cn } from '$lib/utils';

	interface Props {
		eventId: string;
	}

	let { eventId }: Props = $props();

	const { isBookmarked, toggleBookmark } = useBookmarks(eventId);
	const toast = useToast();

	function handleBookmark(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();

		if (!$currentUser) {
			toast.toastError('Please log in to bookmark posts');
			return;
		}

		toggleBookmark.mutate(undefined, {
			onSuccess: () => {
				if (isBookmarked) {
					toast.toastSuccess('Removed from bookmarks');
				} else {
					toast.toastSuccess('Added to bookmarks');
				}
			},
			onError: () => {
				toast.toastError('Failed to update bookmark');
			}
		});
	}
</script>

<Button
	variant="ghost"
	size="sm"
	class={cn(
		'hover:bg-blue-500/10 p-2',
		isBookmarked ? 'text-blue-500' : 'text-muted-foreground hover:text-blue-500'
	)}
	onclick={handleBookmark}
	disabled={toggleBookmark.isPending || !$currentUser}
>
	<Bookmark class={cn('h-[18px] w-[18px]', isBookmarked && 'fill-blue-500')} />
</Button>
