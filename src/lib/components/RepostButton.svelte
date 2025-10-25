<script lang="ts">
	import { Repeat2 } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import { useReposts } from '$lib/hooks/useReposts.svelte';
	import { useToast } from '$lib/stores/toast.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { cn } from '$lib/utils';
	import type { TrustedEvent } from '@welshman/util';

	interface Props {
		event: TrustedEvent;
	}

	let { event }: Props = $props();

	const { reposts, repost } = useReposts(event);
	const toast = useToast();

	function handleRepost(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();

		if (!$currentUser) {
			toast.toastError('Please log in to repost');
			return;
		}

		if (reposts?.hasReposted) {
			toast.toastInfo('You already reposted this');
			return;
		}

		repost.mutate(undefined, {
			onSuccess: () => {
				toast.toastSuccess('Reposted!');
			},
			onError: () => {
				toast.toastError('Failed to repost');
			}
		});
	}
</script>

<Button
	variant="ghost"
	size="sm"
	class={cn(
		'hover:bg-green-500/10 gap-1',
		reposts?.hasReposted ? 'text-green-500' : 'text-muted-foreground hover:text-green-500'
	)}
	onclick={handleRepost}
	disabled={repost.isPending || !$currentUser}
>
	<Repeat2 class="h-[18px] w-[18px]" />
	<span class="text-xs">
		{reposts?.count && reposts.count > 500 ? '500+' : reposts?.count || ''}
	</span>
</Button>
