import { Link } from 'react-router-dom';
import { nip19 } from 'nostr-tools';
import { List, Users, VolumeX, Pin, BookmarkCheck, Hash, ExternalLink, Lock } from 'lucide-react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import { useAuthor } from '@/hooks/useAuthor';

interface ListCardProps {
  kind: number;
  name: string;
  description: string;
  itemCount: number;
  icon?: 'list' | 'users' | 'mute' | 'pin' | 'bookmark';
  isPrivate?: boolean;
  className?: string;
  onClick?: () => void;
}

const iconMap = {
  list: List,
  users: Users,
  mute: VolumeX,
  pin: Pin,
  bookmark: BookmarkCheck,
};

export function ListCard({
  kind,
  name,
  description,
  itemCount,
  icon = 'list',
  isPrivate = false,
  className,
  onClick,
}: ListCardProps) {
  const Icon = iconMap[icon];

  return (
    <Card
      className={cn(
        "cursor-pointer hover:bg-accent/50 transition-colors",
        className
      )}
      onClick={onClick}
    >
      <CardHeader>
        <div className="flex items-start justify-between gap-4">
          <div className="flex items-center gap-3 flex-1">
            <div className="w-12 h-12 rounded-full bg-blue-500/10 flex items-center justify-center flex-shrink-0">
              <Icon className="h-6 w-6 text-blue-500" />
            </div>
            <div className="flex-1 min-w-0">
              <CardTitle className="text-lg flex items-center gap-2">
                {name}
                {isPrivate && <Lock className="h-4 w-4 text-muted-foreground" />}
              </CardTitle>
              <CardDescription className="mt-1">{description}</CardDescription>
            </div>
          </div>
          <Badge variant="secondary" className="flex-shrink-0">
            {itemCount} {itemCount === 1 ? 'item' : 'items'}
          </Badge>
        </div>
      </CardHeader>
    </Card>
  );
}

interface ListItemDisplayProps {
  type: 'p' | 'e' | 'a' | 't' | 'r' | 'word';
  value: string;
  relay?: string;
  onRemove?: () => void;
}

function UserCard({ pubkey, relay }: { pubkey: string; relay?: string }) {
  const { data } = useAuthor(pubkey);
  const metadata = data?.metadata;
  const npub = nip19.npubEncode(pubkey);

  return (
    <Link to={`/${npub}`} className="block">
      <div className="flex items-center gap-3 p-4 hover:bg-accent/50 transition-colors border-b border-border last:border-0">
        <img
          src={metadata?.picture || `https://api.dicebear.com/7.x/identicon/svg?seed=${pubkey}`}
          alt={metadata?.name || 'User'}
          className="w-12 h-12 rounded-full flex-shrink-0"
        />
        <div className="flex-1 min-w-0">
          <div className="font-semibold truncate">
            {metadata?.display_name || metadata?.name || 'Anonymous'}
          </div>
          <div className="text-sm text-muted-foreground truncate">
            @{metadata?.name || npub.slice(0, 12)}
          </div>
          {metadata?.nip05 && (
            <div className="text-xs text-muted-foreground truncate">
              {metadata.nip05}
            </div>
          )}
        </div>
        {relay && (
          <Badge variant="outline" className="flex-shrink-0 text-xs">
            {new URL(relay).hostname}
          </Badge>
        )}
      </div>
    </Link>
  );
}

export function ListItemDisplay({ type, value, relay, onRemove }: ListItemDisplayProps) {
  let content: React.ReactNode;
  let link: string | null = null;

  if (type === 'p') {
    // Pubkey - render as user card
    return <UserCard pubkey={value} relay={relay} />;
  } else if (type === 'e') {
    // Event ID
    const noteId = nip19.noteEncode(value);
    link = `/${noteId}`;
    content = (
      <div className="flex items-center gap-2">
        <Pin className="h-4 w-4 text-muted-foreground" />
        <span className="font-mono text-sm">{noteId.slice(0, 16)}...</span>
      </div>
    );
  } else if (type === 't') {
    // Hashtag
    link = `/t/${value}`;
    content = (
      <div className="flex items-center gap-2">
        <Hash className="h-4 w-4 text-muted-foreground" />
        <span>#{value}</span>
      </div>
    );
  } else if (type === 'r') {
    // URL
    content = (
      <a
        href={value}
        target="_blank"
        rel="noopener noreferrer"
        className="flex items-center gap-2 text-blue-500 hover:underline"
        onClick={(e) => e.stopPropagation()}
      >
        <ExternalLink className="h-4 w-4" />
        <span className="truncate">{value}</span>
      </a>
    );
  } else if (type === 'word') {
    // Muted word
    content = (
      <div className="flex items-center gap-2">
        <VolumeX className="h-4 w-4 text-muted-foreground" />
        <span className="italic">{value}</span>
      </div>
    );
  } else {
    // Other types
    content = (
      <div className="flex items-center gap-2">
        <span className="font-mono text-sm">{value}</span>
      </div>
    );
  }

  const inner = (
    <div className="flex items-center justify-between p-3 hover:bg-accent/50 transition-colors border-b border-border last:border-0">
      <div className="flex-1 min-w-0">{content}</div>
      {relay && (
        <Badge variant="outline" className="ml-2 flex-shrink-0 text-xs">
          {new URL(relay).hostname}
        </Badge>
      )}
    </div>
  );

  if (link) {
    return (
      <Link to={link} className="block">
        {inner}
      </Link>
    );
  }

  return inner;
}
