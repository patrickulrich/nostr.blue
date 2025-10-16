import { nip44 } from 'nostr-tools';

/**
 * Encrypt data using NIP-44 with the author's own keys
 * This creates a shared secret using the author's public and private keys
 */
export async function encryptPrivateItems(
  items: string[][],
  pubkey: string,
  signer: { nip44?: { encrypt?: (pubkey: string, plaintext: string) => Promise<string> } }
): Promise<string> {
  const jsonString = JSON.stringify(items);

  // Try to use the signer's NIP-44 encrypt method if available (for browser extensions)
  if (signer.nip44?.encrypt) {
    return await signer.nip44.encrypt(pubkey, jsonString);
  }

  // If no signer encrypt method, we need the private key
  // This shouldn't happen in production as we always use signers
  throw new Error('NIP-44 encryption requires a signer with encrypt capability');
}

/**
 * Decrypt data using NIP-44 with the author's own keys
 */
export async function decryptPrivateItems(
  ciphertext: string,
  pubkey: string,
  signer: { nip44?: { decrypt?: (pubkey: string, ciphertext: string) => Promise<string> } }
): Promise<string[][]> {
  if (!ciphertext) return [];

  try {
    // Try to use the signer's NIP-44 decrypt method if available (for browser extensions)
    if (signer.nip44?.decrypt) {
      const plaintext = await signer.nip44.decrypt(pubkey, ciphertext);
      return JSON.parse(plaintext);
    }

    // If no signer decrypt method, we can't decrypt
    throw new Error('NIP-44 decryption requires a signer with decrypt capability');
  } catch (error) {
    console.error('Failed to decrypt private items:', error);
    return [];
  }
}

/**
 * Check if content is encrypted (basic heuristic)
 * NIP-44 ciphertext is base64 encoded and typically contains certain patterns
 */
export function isEncrypted(content: string): boolean {
  if (!content) return false;

  // Check if it looks like base64 encrypted content
  // NIP-44 content should be base64 and reasonably long
  return content.length > 20 && /^[A-Za-z0-9+/]+=*$/.test(content);
}
