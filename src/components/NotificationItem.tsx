import { Link } from 'react-router-dom';
import { nip19 } from 'nostr-tools';
import { Heart, Repeat2, MessageCircle, Zap, AtSign } from 'lucide-react';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { type NotificationEvent } from '@/hooks/useNotifications';
import { useAuthor } from '@/hooks/useAuthor';
import { genUserName } from '@/lib/genUserName';
import { formatDistanceToNow } from 'date-fns';
import { cn } from '@/lib/utils';
import { encodeTargetId } from '@/lib/nostrEncoding';

interface NotificationItemProps {
  notification: NotificationEvent;
  className?: string;
}

export function NotificationItem({ notification, className }: NotificationItemProps) {
  const { event, type, targetEventId } = notification;
  const { data: author } = useAuthor(event.pubkey);

  const npub = nip19.npubEncode(event.pubkey);
  const displayName = author?.metadata?.name || genUserName(event.pubkey);
  const avatarUrl = author?.metadata?.picture;
  const timestamp = formatDistanceToNow(new Date(event.created_at * 1000), { addSuffix: true });

  // Encode the target event ID or address for linking
  const targetNoteId = encodeTargetId(targetEventId);

  // Get the appropriate icon and color based on notification type
  const getIcon = () => {
    switch (type) {
      case 'reaction':
        return <Heart className="h-8 w-8 fill-pink-500 text-pink-500" />;
      case 'repost':
        return <Repeat2 className="h-8 w-8 text-green-500" />;
      case 'reply':
        return <MessageCircle className="h-8 w-8 text-blue-500" />;
      case 'zap':
        return <Zap className="h-8 w-8 fill-amber-500 text-amber-500" />;
      case 'mention':
        return <AtSign className="h-8 w-8 text-blue-500" />;
      default:
        return null;
    }
  };

  // Get the notification text
  const getNotificationText = () => {
    switch (type) {
      case 'reaction':
        return 'liked your post';
      case 'repost':
        return 'reposted your post';
      case 'reply':
        return 'replied to your post';
      case 'zap':
        return 'zapped your post';
      case 'mention':
        return 'mentioned you';
      default:
        return 'interacted with your post';
    }
  };

  // Link to the appropriate page
  const linkTo = type === 'reply'
    ? `/${nip19.noteEncode(event.id)}` // Link to the reply itself
    : targetNoteId
    ? `/${targetNoteId}` // Link to the target post
    : `/${npub}`; // Fallback to profile

  return (
    <Link
      to={linkTo}
      className={cn(
        "flex gap-3 p-4 border-b border-border hover:bg-accent/50 transition-colors",
        className
      )}
    >
      {/* Icon */}
      <div className="flex-shrink-0 w-12 flex justify-center pt-1">
        {getIcon()}
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        {/* Avatar and name */}
        <div className="flex items-start gap-2 mb-2">
          <Avatar className="w-8 h-8">
            <AvatarImage src={avatarUrl} alt={displayName} />
            <AvatarFallback className="text-xs">{displayName[0]?.toUpperCase() || 'A'}</AvatarFallback>
          </Avatar>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <span className="font-bold hover:underline">{displayName}</span>
              <span className="text-muted-foreground text-sm">{getNotificationText()}</span>
              <span className="text-muted-foreground text-sm">· {timestamp}</span>
            </div>
          </div>
        </div>

        {/* Content preview (for replies and mentions) */}
        {(type === 'reply' || type === 'mention') && event.content && (
          <div className="mt-2 text-sm text-muted-foreground line-clamp-3 pl-10">
            {event.content}
          </div>
        )}

        {/* Reaction emoji */}
        {type === 'reaction' && (
          <div className="mt-1 text-2xl pl-10">
            {event.content || '❤️'}
          </div>
        )}
      </div>
    </Link>
  );
}
