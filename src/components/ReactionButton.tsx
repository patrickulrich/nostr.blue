import { type NostrEvent } from '@nostrify/nostrify';
import { Heart } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useReactions } from '@/hooks/useReactions';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { cn } from '@/lib/utils';

interface ReactionButtonProps {
  event: NostrEvent;
  className?: string;
}

/**
 * Button component for adding/removing reactions (likes) to a Nostr event.
 * Displays reaction count and allows users to toggle their reaction.
 *
 * @param props - Component properties
 * @param props.event - The Nostr event to react to
 * @param props.className - Optional CSS class names
 */
export function ReactionButton({ event, className }: ReactionButtonProps) {
  const { user } = useCurrentUser();
  const { count, userReacted, addReaction, removeReaction, isLoading } = useReactions(event.id);

  const handleClick = async (e: React.MouseEvent) => {
    e.preventDefault();
    e.stopPropagation();

    if (!user) return;

    if (userReacted) {
      removeReaction.mutate();
    } else {
      addReaction.mutate(event);
    }
  };

  const isPending = addReaction.isPending || removeReaction.isPending;

  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={handleClick}
      disabled={!user || isPending || isLoading}
      className={cn(
        "gap-1 transition-colors",
        userReacted
          ? "text-pink-500 hover:text-pink-600 hover:bg-pink-500/10"
          : "text-muted-foreground hover:text-pink-500 hover:bg-pink-500/10",
        className
      )}
    >
      <Heart
        className={cn(
          "h-4 w-4 transition-all",
          userReacted && "fill-pink-500"
        )}
      />
      {count > 0 && <span className="text-xs">{count}</span>}
    </Button>
  );
}
