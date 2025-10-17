import { type NostrEvent } from '@nostrify/nostrify';
import { Repeat2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useReposts } from '@/hooks/useReposts';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { cn } from '@/lib/utils';

interface RepostButtonProps {
  event: NostrEvent;
  className?: string;
}

/**
 * Button component for reposting/boosting a Nostr event.
 * Displays repost count and allows users to toggle their repost.
 *
 * @param props - Component properties
 * @param props.event - The Nostr event to repost
 * @param props.className - Optional CSS class names
 */
export function RepostButton({ event, className }: RepostButtonProps) {
  const { user } = useCurrentUser();
  const { count, userReposted, addRepost, removeRepost, isLoading } = useReposts(event.id);

  const handleClick = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();

    if (!user) return;

    if (userReposted) {
      removeRepost.mutate();
    } else {
      addRepost.mutate(event);
    }
  };

  const isPending = addRepost.isPending || removeRepost.isPending;

  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={handleClick}
      disabled={!user || isPending || isLoading}
      aria-pressed={userReposted}
      className={cn(
        "gap-1 transition-colors",
        userReposted
          ? "text-green-500 hover:text-green-600 hover:bg-green-500/10"
          : "text-muted-foreground hover:text-green-500 hover:bg-green-500/10",
        className
      )}
    >
      <Repeat2
        className={cn(
          "h-4 w-4 transition-all",
          userReposted && "stroke-[3px]"
        )}
      />
      {count > 0 && <span className="text-xs">{count}</span>}
    </Button>
  );
}
