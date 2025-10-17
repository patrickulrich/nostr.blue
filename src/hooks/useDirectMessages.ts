import { useQuery } from '@tanstack/react-query';
import { useNostr } from '@nostrify/react';
import type { NostrEvent } from '@nostrify/nostrify';
import { useCurrentUser } from './useCurrentUser';

/**
 * Decrypted direct message with metadata.
 */
export interface DecryptedMessage {
  id: string;
  pubkey: string;
  otherPubkey: string; // The other person in the conversation
  content: string;
  created_at: number;
  isSent: boolean; // true if we sent it, false if we received it
  rawEvent: NostrEvent;
}

/**
 * Conversation thread with a specific user.
 */
export interface Conversation {
  pubkey: string; // The other person's pubkey
  lastMessage: DecryptedMessage;
  messages: DecryptedMessage[];
  unreadCount: number;
}

/**
 * Hook to fetch and decrypt direct messages (NIP-04 kind 4)
 * Groups messages into conversations by pubkey
 */
export function useDirectMessages() {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();

  return useQuery({
    queryKey: ['direct-messages', user?.pubkey],
    queryFn: async ({ signal }) => {
      if (!user) return [];

      const userPubkey = user.pubkey;

      // Query both sent and received kind 4 messages
      const events = await nostr.query(
        [
          // Received messages
          { kinds: [4], '#p': [userPubkey], limit: 500 },
          // Sent messages
          { kinds: [4], authors: [userPubkey], limit: 500 },
        ],
        { signal: AbortSignal.any([signal, AbortSignal.timeout(5000)]) }
      );

      // Decrypt messages
      const decryptedMessages: DecryptedMessage[] = [];

      for (const event of events) {
        try {
          // Determine the other party in the conversation
          const isSent = event.pubkey === userPubkey;
          const otherPubkey = isSent
            ? event.tags.find((t) => t[0] === 'p')?.[1]
            : event.pubkey;

          if (!otherPubkey) continue;

          // Decrypt the content
          let decryptedContent: string;
          try {
            // Check if signer has nip04 or nip44 methods
            if (user.signer.nip04) {
              decryptedContent = await user.signer.nip04.decrypt(
                otherPubkey,
                event.content
              );
            } else if (user.signer.nip44) {
              // Fallback to nip44 if nip04 is not available
              decryptedContent = await user.signer.nip44.decrypt(
                otherPubkey,
                event.content
              );
            } else {
              console.warn('Signer does not support encryption');
              continue;
            }
          } catch (decryptError) {
            console.error('Failed to decrypt message:', decryptError);
            // Skip messages that fail to decrypt
            continue;
          }

          decryptedMessages.push({
            id: event.id,
            pubkey: event.pubkey,
            otherPubkey,
            content: decryptedContent,
            created_at: event.created_at,
            isSent,
            rawEvent: event,
          });
        } catch (error) {
          console.error('Error processing message:', error);
        }
      }

      // Group messages by conversation (other person's pubkey)
      const conversationsMap = new Map<string, Conversation>();

      for (const message of decryptedMessages) {
        const { otherPubkey } = message;

        if (!conversationsMap.has(otherPubkey)) {
          conversationsMap.set(otherPubkey, {
            pubkey: otherPubkey,
            lastMessage: message,
            messages: [],
            unreadCount: 0,
          });
        }

        const conversation = conversationsMap.get(otherPubkey)!;
        conversation.messages.push(message);

        // Update last message if this one is newer
        if (message.created_at > conversation.lastMessage.created_at) {
          conversation.lastMessage = message;
        }
      }

      // Sort messages within each conversation by timestamp
      const conversations = Array.from(conversationsMap.values());
      conversations.forEach((conv) => {
        conv.messages.sort((a, b) => a.created_at - b.created_at);
      });

      // Sort conversations by last message timestamp (newest first)
      conversations.sort(
        (a, b) => b.lastMessage.created_at - a.lastMessage.created_at
      );

      return conversations;
    },
    enabled: !!user,
    staleTime: 30000, // Consider data fresh for 30 seconds
    refetchInterval: 60000, // Refetch every minute
  });
}
