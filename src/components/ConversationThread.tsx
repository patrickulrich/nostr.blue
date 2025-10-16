import { useEffect, useRef, useState } from 'react';
import { formatDistanceToNow } from 'date-fns';
import { Send, MessageCircle } from 'lucide-react';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Textarea } from '@/components/ui/textarea';
import { useAuthor } from '@/hooks/useAuthor';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { useSendMessage } from '@/hooks/useSendMessage';
import type { Conversation } from '@/hooks/useDirectMessages';
import { cn } from '@/lib/utils';
import { genUserName } from '@/lib/genUserName';

interface ConversationThreadProps {
  conversation: Conversation;
}

function MessageBubble({
  content,
  timestamp,
  isSent,
  authorPubkey,
}: {
  content: string;
  timestamp: number;
  isSent: boolean;
  authorPubkey: string;
}) {
  const author = useAuthor(authorPubkey);
  const metadata = author.data?.metadata;

  const displayName = metadata?.name || genUserName(authorPubkey);
  const avatarUrl = metadata?.picture;

  const timeAgo = formatDistanceToNow(new Date(timestamp * 1000), {
    addSuffix: true,
  });

  return (
    <div
      className={cn(
        'flex gap-3 mb-4',
        isSent ? 'flex-row-reverse' : 'flex-row'
      )}
    >
      <Avatar className="h-8 w-8 flex-shrink-0">
        <AvatarImage src={avatarUrl} alt={displayName} />
        <AvatarFallback>{displayName.slice(0, 2).toUpperCase()}</AvatarFallback>
      </Avatar>

      <div
        className={cn(
          'flex flex-col gap-1 max-w-[70%]',
          isSent ? 'items-end' : 'items-start'
        )}
      >
        <div
          className={cn(
            'rounded-2xl px-4 py-2 break-words',
            isSent
              ? 'bg-primary text-primary-foreground'
              : 'bg-muted text-foreground'
          )}
        >
          <p className="text-sm whitespace-pre-wrap">{content}</p>
        </div>
        <span className="text-xs text-muted-foreground px-2">{timeAgo}</span>
      </div>
    </div>
  );
}

export function ConversationThread({ conversation }: ConversationThreadProps) {
  const { user } = useCurrentUser();
  const author = useAuthor(conversation.pubkey);
  const metadata = author.data?.metadata;
  const { mutate: sendMessage, isPending: isSending } = useSendMessage();

  const [messageText, setMessageText] = useState('');
  const scrollAreaRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const displayName = metadata?.name || genUserName(conversation.pubkey);
  const avatarUrl = metadata?.picture;

  // Auto-scroll to bottom when messages change
  useEffect(() => {
    if (scrollAreaRef.current) {
      const scrollContainer = scrollAreaRef.current.querySelector(
        '[data-radix-scroll-area-viewport]'
      );
      if (scrollContainer) {
        scrollContainer.scrollTop = scrollContainer.scrollHeight;
      }
    }
  }, [conversation.messages]);

  const handleSendMessage = () => {
    if (!messageText.trim() || !user) return;

    sendMessage(
      {
        recipientPubkey: conversation.pubkey,
        content: messageText.trim(),
      },
      {
        onSuccess: () => {
          setMessageText('');
          textareaRef.current?.focus();
        },
      }
    );
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSendMessage();
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center gap-3 p-4 border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <Avatar className="h-10 w-10">
          <AvatarImage src={avatarUrl} alt={displayName} />
          <AvatarFallback>
            {displayName.slice(0, 2).toUpperCase()}
          </AvatarFallback>
        </Avatar>
        <div>
          <h2 className="font-semibold">{displayName}</h2>
          {metadata?.nip05 && (
            <p className="text-xs text-muted-foreground">{metadata.nip05}</p>
          )}
        </div>
      </div>

      {/* Messages */}
      <ScrollArea ref={scrollAreaRef} className="flex-1 p-4">
        {conversation.messages.map((message) => (
          <MessageBubble
            key={message.id}
            content={message.content}
            timestamp={message.created_at}
            isSent={message.isSent}
            authorPubkey={message.pubkey}
          />
        ))}
      </ScrollArea>

      {/* Input */}
      <div className="p-4 border-t bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
        <div className="flex gap-2">
          <Textarea
            ref={textareaRef}
            placeholder="Type a message..."
            value={messageText}
            onChange={(e) => setMessageText(e.target.value)}
            onKeyDown={handleKeyDown}
            className="min-h-[60px] max-h-[200px] resize-none"
            disabled={isSending}
          />
          <Button
            size="icon"
            onClick={handleSendMessage}
            disabled={!messageText.trim() || isSending}
            className="h-[60px] w-[60px] flex-shrink-0"
          >
            <Send className="h-5 w-5" />
          </Button>
        </div>
        <p className="text-xs text-muted-foreground mt-2">
          Press Enter to send, Shift+Enter for new line
        </p>
      </div>
    </div>
  );
}

export function EmptyConversationThread() {
  return (
    <div className="flex flex-col items-center justify-center h-full p-8 text-center">
      <MessageCircle className="h-16 w-16 text-muted-foreground mb-4" />
      <h3 className="font-semibold text-xl mb-2">Select a conversation</h3>
      <p className="text-sm text-muted-foreground max-w-sm">
        Choose a conversation from the list to start messaging.
      </p>
    </div>
  );
}
