import { formatDistanceToNow } from 'date-fns';
import { MessageCircle } from 'lucide-react';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Skeleton } from '@/components/ui/skeleton';
import { useAuthor } from '@/hooks/useAuthor';
import type { Conversation } from '@/hooks/useDirectMessages';
import { cn } from '@/lib/utils';
import { genUserName } from '@/lib/genUserName';

interface ConversationListProps {
  conversations: Conversation[];
  selectedPubkey?: string;
  onSelectConversation: (pubkey: string) => void;
  isLoading?: boolean;
}

function ConversationItem({
  conversation,
  isSelected,
  onClick,
}: {
  conversation: Conversation;
  isSelected: boolean;
  onClick: () => void;
}) {
  const author = useAuthor(conversation.pubkey);
  const metadata = author.data?.metadata;

  const displayName = metadata?.name || genUserName(conversation.pubkey);
  const avatarUrl = metadata?.picture;

  // Truncate message content for preview
  const messagePreview =
    conversation.lastMessage.content.length > 60
      ? conversation.lastMessage.content.slice(0, 60) + '...'
      : conversation.lastMessage.content;

  const timeAgo = formatDistanceToNow(
    new Date(conversation.lastMessage.created_at * 1000),
    { addSuffix: true }
  );

  return (
    <Button
      variant="ghost"
      className={cn(
        'w-full justify-start h-auto py-3 px-4 hover:bg-muted/50',
        isSelected && 'bg-muted'
      )}
      onClick={onClick}
    >
      <div className="flex items-start gap-3 w-full">
        <Avatar className="h-12 w-12 flex-shrink-0">
          <AvatarImage src={avatarUrl} alt={displayName} />
          <AvatarFallback>
            {displayName.slice(0, 2).toUpperCase()}
          </AvatarFallback>
        </Avatar>

        <div className="flex-1 min-w-0 text-left">
          <div className="flex items-center justify-between gap-2">
            <span className="font-semibold text-sm truncate">
              {displayName}
            </span>
            <span className="text-xs text-muted-foreground flex-shrink-0">
              {timeAgo}
            </span>
          </div>

          <p className="text-sm text-muted-foreground truncate mt-1">
            {conversation.lastMessage.isSent && (
              <span className="text-foreground/70">You: </span>
            )}
            {messagePreview}
          </p>
        </div>
      </div>
    </Button>
  );
}

function ConversationListSkeleton() {
  return (
    <div className="space-y-2 p-2">
      {[...Array(5)].map((_, i) => (
        <div key={i} className="flex items-start gap-3 p-4">
          <Skeleton className="h-12 w-12 rounded-full flex-shrink-0" />
          <div className="flex-1 space-y-2">
            <div className="flex items-center justify-between">
              <Skeleton className="h-4 w-24" />
              <Skeleton className="h-3 w-16" />
            </div>
            <Skeleton className="h-3 w-full" />
          </div>
        </div>
      ))}
    </div>
  );
}

export function ConversationList({
  conversations,
  selectedPubkey,
  onSelectConversation,
  isLoading,
}: ConversationListProps) {
  if (isLoading) {
    return <ConversationListSkeleton />;
  }

  if (conversations.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-full p-8 text-center">
        <MessageCircle className="h-12 w-12 text-muted-foreground mb-4" />
        <h3 className="font-semibold text-lg mb-2">No conversations yet</h3>
        <p className="text-sm text-muted-foreground max-w-sm">
          Start a conversation by visiting someone's profile and sending them a
          message.
        </p>
      </div>
    );
  }

  return (
    <ScrollArea className="h-full">
      <div className="space-y-1 p-2">
        {conversations.map((conversation) => (
          <ConversationItem
            key={conversation.pubkey}
            conversation={conversation}
            isSelected={selectedPubkey === conversation.pubkey}
            onClick={() => onSelectConversation(conversation.pubkey)}
          />
        ))}
      </div>
    </ScrollArea>
  );
}
