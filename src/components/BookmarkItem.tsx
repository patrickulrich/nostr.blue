import { Link } from 'react-router-dom';
import { nip19 } from 'nostr-tools';
import { ExternalLink, Hash, FileText, X } from 'lucide-react';
import { type NostrEvent } from '@nostrify/nostrify';
import { useNostr } from '@nostrify/react';
import { useQuery } from '@tanstack/react-query';
import { PostCard } from '@/components/PostCard';
import { Button } from '@/components/ui/button';
import { type Bookmark } from '@/hooks/useBookmarks';
import { cn } from '@/lib/utils';

interface BookmarkItemProps {
  bookmark: Bookmark;
  onRemove?: () => void;
  className?: string;
}

export function BookmarkItem({ bookmark, onRemove, className }: BookmarkItemProps) {
  const { nostr } = useNostr();

  // Fetch the actual event for note bookmarks
  const { data: noteEvent } = useQuery<NostrEvent | null>({
    queryKey: ['bookmark-event', bookmark.value],
    queryFn: async ({ signal }) => {
      if (bookmark.type !== 'note') return null;

      try {
        const [event] = await nostr.query(
          [{ ids: [bookmark.value], kinds: [1], limit: 1 }],
          { signal: AbortSignal.any([signal, AbortSignal.timeout(3000)]) }
        );
        return event || null;
      } catch (error) {
        console.error('Failed to fetch bookmarked note:', error);
        return null;
      }
    },
    enabled: bookmark.type === 'note',
    staleTime: 60000,
  });

  // Render based on bookmark type
  if (bookmark.type === 'note') {
    if (!noteEvent) {
      // Show skeleton/placeholder while loading
      return (
        <div className={cn("border-b border-border p-4 animate-pulse", className)}>
          <div className="h-20 bg-muted rounded" />
        </div>
      );
    }

    return (
      <div className={cn("relative group", className)}>
        <PostCard event={noteEvent} />
        {onRemove && (
          <Button
            variant="ghost"
            size="icon"
            onClick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onRemove();
            }}
            className="absolute top-2 right-2 opacity-0 group-hover:opacity-100 transition-opacity bg-background/80 hover:bg-destructive hover:text-destructive-foreground"
          >
            <X className="h-4 w-4" />
          </Button>
        )}
      </div>
    );
  }

  if (bookmark.type === 'article') {
    // Parse article coordinate (kind:pubkey:identifier)
    const [kind, pubkey, identifier] = bookmark.value.split(':');
    const naddr = nip19.naddrEncode({
      kind: parseInt(kind),
      pubkey,
      identifier,
    });

    return (
      <div className={cn("border-b border-border p-4 hover:bg-accent/50 transition-colors group", className)}>
        <Link to={`/${naddr}`} className="flex items-start gap-3">
          <div className="flex-shrink-0 w-10 h-10 rounded-lg bg-blue-500/10 flex items-center justify-center">
            <FileText className="h-5 w-5 text-blue-500" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="font-semibold mb-1">Article</div>
            <div className="text-sm text-muted-foreground truncate font-mono">
              {bookmark.value}
            </div>
          </div>
          {onRemove && (
            <Button
              variant="ghost"
              size="icon"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                onRemove();
              }}
              className="opacity-0 group-hover:opacity-100 transition-opacity hover:bg-destructive hover:text-destructive-foreground"
            >
              <X className="h-4 w-4" />
            </Button>
          )}
        </Link>
      </div>
    );
  }

  if (bookmark.type === 'hashtag') {
    return (
      <div className={cn("border-b border-border p-4 hover:bg-accent/50 transition-colors group", className)}>
        <Link to={`/t/${bookmark.value}`} className="flex items-start gap-3">
          <div className="flex-shrink-0 w-10 h-10 rounded-lg bg-blue-500/10 flex items-center justify-center">
            <Hash className="h-5 w-5 text-blue-500" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="font-semibold mb-1">#{bookmark.value}</div>
            <div className="text-sm text-muted-foreground">Hashtag</div>
          </div>
          {onRemove && (
            <Button
              variant="ghost"
              size="icon"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                onRemove();
              }}
              className="opacity-0 group-hover:opacity-100 transition-opacity hover:bg-destructive hover:text-destructive-foreground"
            >
              <X className="h-4 w-4" />
            </Button>
          )}
        </Link>
      </div>
    );
  }

  if (bookmark.type === 'url') {
    return (
      <div className={cn("border-b border-border p-4 hover:bg-accent/50 transition-colors group", className)}>
        <a
          href={bookmark.value}
          target="_blank"
          rel="noopener noreferrer"
          className="flex items-start gap-3"
        >
          <div className="flex-shrink-0 w-10 h-10 rounded-lg bg-blue-500/10 flex items-center justify-center">
            <ExternalLink className="h-5 w-5 text-blue-500" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="font-semibold mb-1 truncate">{bookmark.value}</div>
            <div className="text-sm text-muted-foreground">External Link</div>
          </div>
          {onRemove && (
            <Button
              variant="ghost"
              size="icon"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                onRemove();
              }}
              className="opacity-0 group-hover:opacity-100 transition-opacity hover:bg-destructive hover:text-destructive-foreground"
            >
              <X className="h-4 w-4" />
            </Button>
          )}
        </a>
      </div>
    );
  }

  return null;
}
