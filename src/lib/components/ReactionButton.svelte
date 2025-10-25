<script lang="ts">
	import { Heart } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import { useReactions } from '$lib/hooks/useReactions.svelte';
	import { useToast } from '$lib/stores/toast.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { cn } from '$lib/utils';

	interface Props {
		eventId: string;
		authorPubkey: string;
	}

	let { eventId, authorPubkey }: Props = $props();

	const { reactions, react } = useReactions(eventId, authorPubkey);
	const toast = useToast();

	function handleReact(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();

		if (!$currentUser) {
			toast.toastError('Please log in to react to posts');
			return;
		}

		if (reactions?.hasReacted) {
			toast.toastInfo('You already reacted to this post');
			return;
		}

		react.mutate(undefined, {
			onSuccess: () => {
				toast.toastSuccess('Reacted!');
			},
			onError: () => {
				toast.toastError('Failed to react');
			}
		});
	}
</script>

<Button
	variant="ghost"
	size="sm"
	class={cn(
		'hover:bg-red-500/10 gap-1 -ml-2',
		reactions?.hasReacted ? 'text-red-500' : 'text-muted-foreground hover:text-red-500'
	)}
	onclick={handleReact}
	disabled={react.isPending || !$currentUser}
>
	<Heart class={cn('h-[18px] w-[18px]', reactions?.hasReacted && 'fill-red-500')} />
	<span class="text-xs">
		{reactions?.count && reactions.count > 500 ? '500+' : reactions?.count || ''}
	</span>
</Button>
