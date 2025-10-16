import { useState } from 'react';
import { Loader2, MessageCircle, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Card } from '@/components/ui/card';
import { Separator } from '@/components/ui/separator';
import { ConversationList } from '@/components/ConversationList';
import {
  ConversationThread,
  EmptyConversationThread,
} from '@/components/ConversationThread';
import { LoginArea } from '@/components/auth/LoginArea';
import { MainLayout } from '@/components/MainLayout';
import { useCurrentUser } from '@/hooks/useCurrentUser';
import { useDirectMessages } from '@/hooks/useDirectMessages';
import { useIsMobile } from '@/hooks/useIsMobile';

export default function MessagesPage() {
  const { user } = useCurrentUser();
  const { data: conversations, isLoading, refetch, isRefetching } = useDirectMessages();
  const [selectedPubkey, setSelectedPubkey] = useState<string | undefined>();
  const [showThread, setShowThread] = useState(false);
  const isMobile = useIsMobile();

  const selectedConversation = conversations?.find(
    (c) => c.pubkey === selectedPubkey
  );

  const handleSelectConversation = (pubkey: string) => {
    setSelectedPubkey(pubkey);
    if (isMobile) {
      setShowThread(true);
    }
  };

  const handleBackToList = () => {
    setShowThread(false);
  };

  // Not logged in
  if (!user) {
    return (
      <MainLayout>
        <div className="container max-w-4xl py-8">
          <Card className="p-8 text-center">
            <MessageCircle className="h-16 w-16 mx-auto mb-4 text-muted-foreground" />
            <h2 className="text-2xl font-bold mb-2">Private Messages</h2>
            <p className="text-muted-foreground mb-6">
              Sign in to send and receive encrypted direct messages on Nostr.
            </p>
            <div className="flex justify-center">
              <LoginArea className="max-w-xs" />
            </div>
          </Card>
        </div>
      </MainLayout>
    );
  }

  // Mobile view - show either list or thread
  if (isMobile) {
    if (showThread && selectedConversation) {
      return (
        <MainLayout>
          <div className="h-[calc(100vh-4rem)]">
            <div className="flex items-center gap-2 p-4 border-b">
              <Button variant="ghost" size="sm" onClick={handleBackToList}>
                ← Back
              </Button>
            </div>
            <div className="h-[calc(100%-4rem)]">
              <ConversationThread conversation={selectedConversation} />
            </div>
          </div>
        </MainLayout>
      );
    }

    return (
      <MainLayout>
        <div className="h-[calc(100vh-4rem)]">
          {/* Header */}
          <div className="sticky top-0 z-10 bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60 border-b">
            <div className="flex items-center justify-between p-4">
              <div className="flex items-center gap-2">
                <MessageCircle className="h-5 w-5" />
                <h1 className="text-xl font-bold">Messages</h1>
              </div>
              <Button
                variant="ghost"
                size="icon"
                onClick={() => refetch()}
                disabled={isRefetching}
              >
                {isRefetching ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : (
                  <RefreshCw className="h-4 w-4" />
                )}
              </Button>
            </div>
          </div>

          {/* Conversation List */}
          <div className="h-[calc(100%-5rem)]">
            {isLoading ? (
              <div className="flex items-center justify-center h-full">
                <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <ConversationList
                conversations={conversations || []}
                selectedPubkey={selectedPubkey}
                onSelectConversation={handleSelectConversation}
              />
            )}
          </div>
        </div>
      </MainLayout>
    );
  }

  // Desktop view - two column layout
  return (
    <MainLayout>
      <div className="container max-w-7xl py-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div className="flex items-center gap-2">
            <MessageCircle className="h-6 w-6" />
            <h1 className="text-2xl font-bold">Messages</h1>
          </div>
          <Button
            variant="outline"
            size="sm"
            onClick={() => refetch()}
            disabled={isRefetching}
          >
            {isRefetching ? (
              <Loader2 className="h-4 w-4 animate-spin mr-2" />
            ) : (
              <RefreshCw className="h-4 w-4 mr-2" />
            )}
            Refresh
          </Button>
        </div>

        {/* Two column layout */}
        <Card className="h-[calc(100vh-12rem)] flex overflow-hidden">
          {/* Left: Conversation List */}
          <div className="w-80 border-r flex flex-col">
            {isLoading ? (
              <div className="flex items-center justify-center h-full">
                <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
              </div>
            ) : (
              <ConversationList
                conversations={conversations || []}
                selectedPubkey={selectedPubkey}
                onSelectConversation={handleSelectConversation}
              />
            )}
          </div>

          <Separator orientation="vertical" />

          {/* Right: Conversation Thread */}
          <div className="flex-1">
            {selectedConversation ? (
              <ConversationThread conversation={selectedConversation} />
            ) : (
              <EmptyConversationThread />
            )}
          </div>
        </Card>
      </div>
    </MainLayout>
  );
}
