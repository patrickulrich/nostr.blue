import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNostr } from '@nostrify/react';
import { useCurrentUser } from './useCurrentUser';

export interface SendMessageParams {
  recipientPubkey: string;
  content: string;
}

/**
 * Hook to send encrypted direct messages (NIP-04 kind 4)
 * Uses NIP-04 encryption for maximum compatibility
 */
export function useSendMessage() {
  const { nostr } = useNostr();
  const { user } = useCurrentUser();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: async ({ recipientPubkey, content }: SendMessageParams) => {
      if (!user) {
        throw new Error('Must be logged in to send messages');
      }

      // Encrypt the message content
      let encryptedContent: string;

      try {
        // Prefer nip04 for compatibility, fallback to nip44
        if (user.signer.nip04) {
          encryptedContent = await user.signer.nip04.encrypt(
            recipientPubkey,
            content
          );
        } else if (user.signer.nip44) {
          encryptedContent = await user.signer.nip44.encrypt(
            recipientPubkey,
            content
          );
        } else {
          throw new Error('Signer does not support encryption');
        }
      } catch (error) {
        console.error('Encryption failed:', error);
        throw new Error('Failed to encrypt message');
      }

      // Create the event
      const event = await user.signer.signEvent({
        kind: 4,
        content: encryptedContent,
        tags: [['p', recipientPubkey]],
        created_at: Math.floor(Date.now() / 1000),
      });

      // Publish to relays
      await nostr.event(event, {
        signal: AbortSignal.timeout(5000),
      });

      return event;
    },
    onSuccess: () => {
      // Invalidate the direct messages query to refetch
      queryClient.invalidateQueries({ queryKey: ['direct-messages'] });
    },
  });
}
