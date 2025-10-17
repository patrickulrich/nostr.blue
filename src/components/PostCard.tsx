import { type NostrEvent } from '@nostrify/nostrify';
import { Link, useNavigate } from 'react-router-dom';
import { nip19 } from 'nostr-tools';
import { MessageCircle, MoreHorizontal, Share, Bookmark } from 'lucide-react';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { NoteContent } from '@/components/NoteContent';
import { ZapButton } from '@/components/ZapButton';
import { ReactionButton } from '@/components/ReactionButton';
import { RepostButton } from '@/components/RepostButton';
import { useAuthor } from '@/hooks/useAuthor';
import { useBookmarks } from '@/hooks/useBookmarks';
import { useReplyCount } from '@/hooks/useReplyCount';
import { genUserName } from '@/lib/genUserName';
import { cn } from '@/lib/utils';
import { formatDistanceToNow } from 'date-fns';
import { useToast } from '@/hooks/useToast';

interface PostCardProps {
  event: NostrEvent;
  className?: string;
  showThread?: boolean;
}

export function PostCard({ event, className, showThread: _showThread = true }: PostCardProps) {
  const navigate = useNavigate();
  const { data: author } = useAuthor(event.pubkey);
  const { isBookmarked, toggleBookmark } = useBookmarks();
  const { data: replyCount = 0 } = useReplyCount(event.id);
  const { toast } = useToast();
  const npub = nip19.npubEncode(event.pubkey);
  const noteId = nip19.noteEncode(event.id);

  const displayName = author?.metadata?.name || genUserName(event.pubkey);
  const username = author?.metadata?.name || `@${npub.slice(0, 12)}...`;
  const avatarUrl = author?.metadata?.picture;

  const timestamp = formatDistanceToNow(new Date(event.created_at * 1000), { addSuffix: true });
  const bookmarked = isBookmarked(event.id);

  const handleBookmarkClick = async () => {
    try {
      await toggleBookmark.mutateAsync({ eventId: event.id });
      toast({
        title: bookmarked ? 'Bookmark removed' : 'Bookmark added',
        description: bookmarked
          ? 'Post removed from your bookmarks.'
          : 'Post added to your bookmarks.',
      });
    } catch (error) {
      console.error('Failed to toggle bookmark:', error);
      toast({
        title: 'Error',
        description: 'Failed to update bookmark. Please try again.',
        variant: 'destructive',
      });
    }
  };

  const handleCardClick = (e: React.MouseEvent) => {
    // Don't navigate if clicking on a button, link, or interactive element
    const target = e.target as HTMLElement;
    if (
      target.closest('button') ||
      target.closest('a') ||
      target.closest('[role="button"]')
    ) {
      return;
    }
    navigate(`/${noteId}`);
  };

  return (
    <article
      className={cn("border-b border-border hover:bg-accent/50 transition-colors cursor-pointer", className)}
      onClick={handleCardClick}
    >
      <div className="flex gap-3 p-4">
        {/* Avatar */}
        <Link to={`/${npub}`} className="flex-shrink-0">
          <Avatar className="w-12 h-12">
            <AvatarImage src={avatarUrl} alt={displayName} />
            <AvatarFallback>{displayName[0]?.toUpperCase() || 'A'}</AvatarFallback>
          </Avatar>
        </Link>

        {/* Content */}
        <div className="flex-1 min-w-0">
          {/* Header */}
          <div className="flex items-start justify-between gap-2 mb-1">
            <div className="flex items-center gap-2 flex-wrap min-w-0">
              <Link to={`/${npub}`} className="font-bold hover:underline truncate">
                {displayName}
              </Link>
              <Link to={`/${npub}`} className="text-muted-foreground text-sm truncate">
                {username}
              </Link>
              <span className="text-muted-foreground text-sm">·</span>
              <Link
                to={`/${noteId}`}
                className="text-muted-foreground text-sm hover:underline"
                onClick={(e) => e.stopPropagation()}
              >
                {timestamp}
              </Link>
            </div>
            <Button variant="ghost" size="icon" className="flex-shrink-0 -mt-1 -mr-2">
              <MoreHorizontal className="h-4 w-4" />
            </Button>
          </div>

          {/* Post Content */}
          <div className="block">
            <NoteContent event={event} className="text-base mb-3" />
          </div>

          {/* Actions */}
          <div className="flex items-center justify-between max-w-lg mt-2">
            <Button
              variant="ghost"
              size="sm"
              className="text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 gap-1 -ml-2"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                // TODO: Open reply dialog
              }}
            >
              <MessageCircle className="h-[18px] w-[18px]" />
              <span className="text-xs">
                {replyCount > 500 ? '500+' : replyCount > 0 ? replyCount : ''}
              </span>
            </Button>

            <RepostButton event={event} />

            <ReactionButton event={event} />

            <ZapButton target={event as NostrEvent} />

            <div className="flex items-center gap-0">
              <Button
                variant="ghost"
                size="sm"
                className={cn(
                  "hover:text-blue-500 hover:bg-blue-500/10 p-2",
                  bookmarked ? "text-blue-500" : "text-muted-foreground"
                )}
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  handleBookmarkClick();
                }}
                disabled={toggleBookmark.isPending}
              >
                <Bookmark className={cn(
                  "h-[18px] w-[18px]",
                  bookmarked && "fill-blue-500",
                  toggleBookmark.isPending && "animate-pulse"
                )} />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                className="text-muted-foreground hover:text-blue-500 hover:bg-blue-500/10 p-2"
                onClick={(e) => {
                  e.preventDefault();
                  e.stopPropagation();
                  // TODO: Share
                }}
              >
                <Share className="h-[18px] w-[18px]" />
              </Button>
            </div>
          </div>
        </div>
      </div>
    </article>
  );
}
