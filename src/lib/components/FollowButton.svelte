<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { useFollowing } from '$lib/hooks/useFollowing.svelte';
	import { currentUser } from '$lib/stores/auth';
	import { Loader2 } from 'lucide-svelte';
	import { cn } from '$lib/utils';

	let {
		pubkey,
		class: className,
		variant = 'default'
	}: {
		pubkey: string;
		class?: string;
		variant?: 'default' | 'outline' | 'ghost' | 'secondary' | 'destructive' | 'link';
	} = $props();

	const { isFollowing, follow, unfollow, isLoading } = useFollowing();

	// Don't show follow button for current user's own profile
	let shouldShow = $derived($currentUser && $currentUser.pubkey !== pubkey);

	let following = $derived(isFollowing(pubkey));
	let isPending = $derived(follow.isPending || unfollow.isPending);

	function handleClick(e: MouseEvent) {
		e.preventDefault();
		e.stopPropagation();

		if (following) {
			unfollow.mutate(pubkey);
		} else {
			follow.mutate(pubkey);
		}
	}
</script>

{#if shouldShow}
	{#if isLoading}
		<Button {variant} disabled class={cn('rounded-full px-6', className)}>
			<Loader2 class="h-4 w-4 animate-spin" />
		</Button>
	{:else}
		<Button
			onclick={handleClick}
			disabled={isPending}
			variant={following ? 'outline' : variant}
			class={cn(
				'rounded-full px-6 font-bold transition-colors',
				following &&
					'hover:bg-destructive hover:text-destructive-foreground hover:border-destructive',
				className
			)}
		>
			{#if isPending}
				<Loader2 class="h-4 w-4 animate-spin" />
			{:else if following}
				<span>Following</span>
			{:else}
				Follow
			{/if}
		</Button>
	{/if}
{/if}
