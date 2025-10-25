import type { TrustedEvent } from '@welshman/util';

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
	rawEvent: TrustedEvent;
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
 * Decrypt a direct message using NIP-04 or NIP-44
 */
export async function decryptMessage(
	signer: any,
	otherPubkey: string,
	encryptedContent: string
): Promise<string> {
	try {
		// Try NIP-04 first (most compatible)
		if (signer.nip04?.decrypt) {
			return await signer.nip04.decrypt(otherPubkey, encryptedContent);
		}
		// Fallback to NIP-44
		if (signer.nip44?.decrypt) {
			return await signer.nip44.decrypt(otherPubkey, encryptedContent);
		}
		throw new Error('Signer does not support message encryption');
	} catch (error) {
		console.error('Failed to decrypt message:', error);
		throw error;
	}
}

/**
 * Encrypt a message using NIP-04 or NIP-44
 */
export async function encryptMessage(
	signer: any,
	recipientPubkey: string,
	content: string
): Promise<string> {
	try {
		// Prefer NIP-04 for compatibility
		if (signer.nip04?.encrypt) {
			return await signer.nip04.encrypt(recipientPubkey, content);
		}
		// Fallback to NIP-44
		if (signer.nip44?.encrypt) {
			return await signer.nip44.encrypt(recipientPubkey, content);
		}
		throw new Error('Signer does not support message encryption');
	} catch (error) {
		console.error('Failed to encrypt message:', error);
		throw error;
	}
}

/**
 * Group messages into conversations by participant
 */
export function groupMessagesIntoConversations(
	messages: DecryptedMessage[]
): Conversation[] {
	const conversationsMap = new Map<string, Conversation>();

	for (const message of messages) {
		const { otherPubkey } = message;

		if (!conversationsMap.has(otherPubkey)) {
			conversationsMap.set(otherPubkey, {
				pubkey: otherPubkey,
				lastMessage: message,
				messages: [],
				unreadCount: 0
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
	conversations.sort((a, b) => b.lastMessage.created_at - a.lastMessage.created_at);

	return conversations;
}
