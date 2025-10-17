import { useState, useRef } from 'react';
import { Avatar, AvatarFallback, AvatarImage } from '@/components/ui/avatar';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { EmojiPicker } from '@/components/EmojiPicker';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { useNostrPublish } from '@/hooks/useNostrPublish';
import { useUploadFile } from '@/hooks/useUploadFile';
import { useSearchUsers, type UserSearchResult } from '@/hooks/useSearchUsers';
import { genUserName } from '@/lib/genUserName';
import { Loader2, Image, Smile, X } from 'lucide-react';
import { useToast } from '@/hooks/useToast';
import { nip19 } from 'nostr-tools';
import { cn } from '@/lib/utils';

interface PostComposerProps {
  onSuccess?: () => void;
  placeholder?: string;
  replyTo?: string; // Event ID to reply to
  autoFocus?: boolean;
}

interface UploadedImage {
  url: string;
  tags: string[][];
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
  const [uploadedImages, setUploadedImages] = useState<UploadedImage[]>([]);
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);

  const { user, metadata } = useCurrentUser();
  const publish = useNostrPublish();
  const { mutateAsync: uploadFile, isPending: isUploading } = useUploadFile();
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

  // Handle file upload
  const handleFileUpload = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    // Check if it's an image
    if (!file.type.startsWith('image/')) {
      toast({
        title: "Invalid file type",
        description: "Please upload an image file.",
        variant: "destructive",
      });
      return;
    }

    try {
      // Upload the file and get NIP-94 compatible tags
      const tags = await uploadFile(file);
      const url = tags[0][1]; // First tag contains the URL

      setUploadedImages(prev => [...prev, { url, tags }]);

      toast({
        title: "Image uploaded",
        description: "Your image has been uploaded successfully.",
      });
    } catch (error) {
      console.error('Failed to upload image:', error);
      toast({
        title: "Upload failed",
        description: "Failed to upload image. Please try again.",
        variant: "destructive",
      });
    }

    // Reset file input
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  // Remove uploaded image
  const removeImage = (index: number) => {
    setUploadedImages(prev => prev.filter((_, i) => i !== index));
  };

  // Insert emoji at cursor position
  const handleEmojiSelect = (emoji: string) => {
    if (!textareaRef.current) return;

    const textarea = textareaRef.current;
    const start = textarea.selectionStart;
    const end = textarea.selectionEnd;

    const newContent = content.slice(0, start) + emoji + content.slice(end);
    setContent(newContent);

    // Set cursor position after the emoji
    setTimeout(() => {
      textarea.focus();
      const newCursorPos = start + emoji.length;
      textarea.setSelectionRange(newCursorPos, newCursorPos);
    }, 0);
  };

  const handlePost = async () => {
    if (!content.trim() && uploadedImages.length === 0) return;

    try {
      const tags: string[][] = [];

      // Add reply tag if replying
      if (replyTo) {
        tags.push(['e', replyTo, '', 'reply']);
      }

      // Build content with image URLs appended
      let postContent = content.trim();

      // Add imeta tags for each uploaded image
      uploadedImages.forEach(image => {
        // Append URL to content
        if (postContent) {
          postContent += '\n\n';
        }
        postContent += image.url;

        // Add imeta tag for the image
        const imetaTag = ['imeta', ...image.tags.flat()];
        tags.push(imetaTag);
      });

      await publish.mutateAsync({
        kind: 1,
        content: postContent,
        tags,
        created_at: Math.floor(Date.now() / 1000),
      });

      setContent('');
      setUploadedImages([]);
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

        {/* Image Previews */}
        {uploadedImages.length > 0 && (
          <div className="flex flex-wrap gap-2 py-2">
            {uploadedImages.map((image, index) => (
              <div key={index} className="relative group">
                <img
                  src={image.url}
                  alt={`Upload ${index + 1}`}
                  className="h-24 w-24 object-cover rounded-lg border border-border"
                />
                <Button
                  type="button"
                  variant="destructive"
                  size="icon"
                  className="absolute -top-2 -right-2 h-6 w-6 rounded-full opacity-0 group-hover:opacity-100 transition-opacity"
                  onClick={() => removeImage(index)}
                >
                  <X className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
        )}

        {/* Hidden file input */}
        <input
          ref={fileInputRef}
          type="file"
          accept="image/*"
          className="hidden"
          onChange={handleFileUpload}
        />

        {/* Actions */}
        <div className="flex items-center justify-between pt-2 border-t border-border">
          <div className="flex items-center gap-1">
            <Button
              type="button"
              variant="ghost"
              size="icon"
              className="text-primary"
              onClick={() => fileInputRef.current?.click()}
              disabled={isUploading}
            >
              {isUploading ? (
                <Loader2 className="h-5 w-5 animate-spin" />
              ) : (
                <Image className="h-5 w-5" />
              )}
            </Button>
            <EmojiPicker onEmojiSelect={handleEmojiSelect}>
              <Button type="button" variant="ghost" size="icon" className="text-primary">
                <Smile className="h-5 w-5" />
              </Button>
            </EmojiPicker>
          </div>

          <div className="flex items-center gap-3">
            {content.length > 0 && (
              <span className={`text-sm ${isOverLimit ? 'text-destructive' : showWarning ? 'text-yellow-500' : 'text-muted-foreground'}`}>
                {remaining}
              </span>
            )}
            <Button
              onClick={handlePost}
              disabled={(!content.trim() && uploadedImages.length === 0) || isOverLimit || publish.isPending || isUploading}
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
