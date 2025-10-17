import { Button } from '@/components/ui/button';
import { useFollowing } from '@/hooks/useFollowing';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { Loader2 } from 'lucide-react';
import { cn } from '@/lib/utils';

interface FollowButtonProps {
  pubkey: string;
  className?: string;
  variant?: 'default' | 'outline';
}

/**
 * Button component for following/unfollowing a Nostr user.
 * Updates the user's contact list (kind 3) when clicked.
 *
 * @param props - Component properties
 * @param props.pubkey - The public key of the user to follow/unfollow
 * @param props.className - Optional CSS class names
 * @param props.variant - Button variant (default: 'default')
 */
export function FollowButton({ pubkey, className, variant = 'default' }: FollowButtonProps) {
  const { user } = useCurrentUser();
  const { isFollowing, follow, unfollow, isLoading } = useFollowing();

  // Don't show follow button for current user's own profile
  if (!user || user.pubkey === pubkey) {
    return null;
  }

  const following = isFollowing(pubkey);
  const isPending = follow.isPending || unfollow.isPending;

  const handleClick = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();

    if (following) {
      unfollow.mutate(pubkey);
    } else {
      follow.mutate(pubkey);
    }
  };

  if (isLoading) {
    return (
      <Button
        variant={variant}
        disabled
        className={cn("rounded-full px-6", className)}
      >
        <Loader2 className="h-4 w-4 animate-spin" />
      </Button>
    );
  }

  return (
    <Button
      onClick={handleClick}
      disabled={isPending}
      variant={following ? 'outline' : variant}
      className={cn(
        "rounded-full px-6 font-bold transition-colors",
        following && "hover:bg-destructive hover:text-destructive-foreground hover:border-destructive",
        className
      )}
    >
      {isPending ? (
        <Loader2 className="h-4 w-4 animate-spin" />
      ) : following ? (
        <span className="group-hover:hidden">Following</span>
      ) : (
        'Follow'
      )}
    </Button>
  );
}
