import { useState, useRef, useEffect } from 'react';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { useNostrPublish } from '@/hooks/useNostrPublish';
import { useSearchUsers, type UserSearchResult } from '@/hooks/useSearchUsers';
import { genUserName } from '@/lib/genUserName';
import { Loader2, Image, Smile } from 'lucide-react';
import { useToast } from '@/hooks/useToast';
import { nip19 } from 'nostr-tools';
import { cn } from '@/lib/utils';

interface PostComposerProps {
  onSuccess?: () => void;
  placeholder?: string;
  replyTo?: string; // Event ID to reply to
  autoFocus?: boolean;
}

export function PostComposer({
  onSuccess,
  placeholder = "What's happening?",
  replyTo,
  autoFocus = false
}: PostComposerProps) {
  const [content, setContent] = useState('');
  const [mentionQuery, setMentionQuery] = useState('');
  const [mentionStart, setMentionStart] = useState<number | null>(null);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  const { user, metadata } = useCurrentUser();
  const publish = useNostrPublish();
  const { toast } = useToast();
  const { data: userSuggestions, isLoading: isSearching } = useSearchUsers(mentionQuery);

  const displayName = metadata?.name || genUserName(user?.pubkey || '');
  const avatarUrl = metadata?.picture;

  // Handle text change and detect @ mentions
  const handleContentChange = (e: React.ChangeEvent<HTMLTextAreaElement>) => {
    const newContent = e.target.value;
    const cursorPos = e.target.selectionStart;

    setContent(newContent);

    // Check if we're typing a mention
    const textBeforeCursor = newContent.slice(0, cursorPos);
    const mentionMatch = textBeforeCursor.match(/@(\w*)$/);

    if (mentionMatch) {
      setMentionQuery(mentionMatch[1]);
      setMentionStart(cursorPos - mentionMatch[0].length);
      setSelectedIndex(0);
    } else {
      setMentionQuery('');
      setMentionStart(null);
    }
  };

  // Handle mention selection
  const selectMention = (user: UserSearchResult) => {
    if (mentionStart === null) return;

    const nprofile = nip19.nprofileEncode({
      pubkey: user.pubkey,
    });

    const before = content.slice(0, mentionStart);
    const after = content.slice(mentionStart + mentionQuery.length + 1); // +1 for @
    const newContent = `${before}nostr:${nprofile} ${after}`;

    setContent(newContent);
    setMentionQuery('');
    setMentionStart(null);

    // Focus back on textarea
    setTimeout(() => {
      textareaRef.current?.focus();
      const newCursorPos = before.length + nprofile.length + 7; // 7 for "nostr:" + space
      textareaRef.current?.setSelectionRange(newCursorPos, newCursorPos);
    }, 0);
  };

  // Handle keyboard navigation in autocomplete
  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (mentionQuery && userSuggestions && userSuggestions.length > 0) {
      if (e.key === 'ArrowDown') {
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % userSuggestions.length);
      } else if (e.key === 'ArrowUp') {
        e.preventDefault();
        setSelectedIndex((prev) => (prev - 1 + userSuggestions.length) % userSuggestions.length);
      } else if (e.key === 'Enter' || e.key === 'Tab') {
        e.preventDefault();
        selectMention(userSuggestions[selectedIndex]);
      } else if (e.key === 'Escape') {
        setMentionQuery('');
        setMentionStart(null);
      }
    }
  };

  const handlePost = async () => {
    if (!content.trim()) return;

    try {
      const tags: string[][] = [];

      // Add reply tag if replying
      if (replyTo) {
        tags.push(['e', replyTo, '', 'reply']);
      }

      await publish.mutateAsync({
        kind: 1,
        content: content.trim(),
        tags,
        created_at: Math.floor(Date.now() / 1000),
      });

      setContent('');
      toast({
        title: "Posted!",
        description: "Your post has been published to the network.",
      });
      onSuccess?.();
    } catch (error) {
      console.error('Failed to post:', error);
      toast({
        title: "Error",
        description: "Failed to publish your post. Please try again.",
        variant: "destructive",
      });
    }
  };

  if (!user) {
    return (
      <div className="p-4 text-center text-muted-foreground">
        Please log in to post
      </div>
    );
  }

  const maxLength = 280;
  const remaining = maxLength - content.length;
  const isOverLimit = remaining < 0;
  const showWarning = remaining < 20 && remaining >= 0;

  return (
    <div className="flex gap-3 p-4">
      {/* Avatar */}
      <Avatar className="w-12 h-12 flex-shrink-0">
        <AvatarImage src={avatarUrl} alt={displayName} />
        <AvatarFallback>{displayName[0]?.toUpperCase() || 'A'}</AvatarFallback>
      </Avatar>

      {/* Composer */}
      <div className="flex-1 flex flex-col gap-3 relative">
        <Textarea
          ref={textareaRef}
          value={content}
          onChange={handleContentChange}
          onKeyDown={handleKeyDown}
          placeholder={placeholder}
          className="min-h-[100px] text-lg border-none resize-none focus-visible:ring-0 focus-visible:ring-offset-0 p-0"
          autoFocus={autoFocus}
        />

        {/* Mention Autocomplete Dropdown */}
        {mentionQuery && userSuggestions && userSuggestions.length > 0 && (
          <div className="absolute top-full left-0 mt-1 w-full max-w-sm bg-popover border border-border rounded-lg shadow-lg z-50 max-h-64 overflow-y-auto">
            {userSuggestions.map((user, index) => (
              <button
                key={user.pubkey}
                onClick={() => selectMention(user)}
                className={cn(
                  "w-full flex items-center gap-3 px-4 py-3 hover:bg-accent transition-colors text-left",
                  index === selectedIndex && "bg-accent"
                )}
              >
                {user.picture ? (
                  <img src={user.picture} alt={user.name} className="w-10 h-10 rounded-full" />
                ) : (
                  <div className="w-10 h-10 rounded-full bg-muted flex items-center justify-center">
                    {(user.name || user.displayName || 'A')[0].toUpperCase()}
                  </div>
                )}
                <div className="flex-1 min-w-0">
                  <div className="font-semibold truncate">
                    {user.displayName || user.name || genUserName(user.pubkey)}
                  </div>
                  {user.name && user.name !== user.displayName && (
                    <div className="text-sm text-muted-foreground truncate">@{user.name}</div>
                  )}
                  {user.nip05 && (
                    <div className="text-xs text-muted-foreground truncate">{user.nip05}</div>
                  )}
                </div>
              </button>
            ))}
          </div>
        )}

        {isSearching && mentionQuery && (
          <div className="absolute top-full left-0 mt-1 w-full max-w-sm bg-popover border border-border rounded-lg shadow-lg z-50 px-4 py-3">
            <div className="flex items-center gap-2 text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
              <span className="text-sm">Searching users...</span>
            </div>
          </div>
        )}

        {/* Actions */}
        <div className="flex items-center justify-between pt-2 border-t border-border">
          <div className="flex items-center gap-1">
            <Button variant="ghost" size="icon" className="text-primary" disabled>
              <Image className="h-5 w-5" />
            </Button>
            <Button variant="ghost" size="icon" className="text-primary" disabled>
              <Smile className="h-5 w-5" />
            </Button>
          </div>

          <div className="flex items-center gap-3">
            {content.length > 0 && (
              <span className={`text-sm ${isOverLimit ? 'text-destructive' : showWarning ? 'text-yellow-500' : 'text-muted-foreground'}`}>
                {remaining}
              </span>
            )}
            <Button
              onClick={handlePost}
              disabled={!content.trim() || isOverLimit || publish.isPending}
              className="rounded-full px-6"
            >
              {publish.isPending ? (
                <>
                  <Loader2 className="mr-2 h-4 w-4 animate-spin" />
                  Posting...
                </>
              ) : (
                'Post'
              )}
            </Button>
          </div>
        </div>
      </div>
    </div>
  );
}
