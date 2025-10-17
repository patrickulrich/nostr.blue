import { nip19 } from 'nostr-tools';

/**
 * Encodes a target event ID or address for linking.
 * Handles both regular event IDs (hex) and addressable event addresses (kind:pubkey:identifier).
 *
 * @param targetEventId - Event ID in hex format or address format (kind:pubkey:identifier)
 * @returns Encoded nip19 string (note1... or naddr1...) or null if invalid
 */
export function encodeTargetId(targetEventId: string | undefined): string | null {
  if (!targetEventId) return null;

  // Check if it's an address format (kind:pubkey:identifier)
  if (targetEventId.includes(':')) {
    const parts = targetEventId.split(':');
    if (parts.length !== 3) {
      console.warn('[encodeTargetId] Invalid address format:', targetEventId);
      return null;
    }

    const [kindStr, pubkey, identifier] = parts;
    const kind = parseInt(kindStr, 10);

    // Validate addressable event kind range (30000-39999)
    if (isNaN(kind) || kind < 30000 || kind > 39999) {
      console.warn('[encodeTargetId] Invalid addressable kind:', kind);
      return null;
    }

    // Validate pubkey is 64-character hex
    if (!/^[0-9a-f]{64}$/i.test(pubkey)) {
      console.warn('[encodeTargetId] Invalid pubkey format:', pubkey);
      return null;
    }

    try {
      return nip19.naddrEncode({ kind, pubkey, identifier });
    } catch (error) {
      console.error('[encodeTargetId] Failed to encode naddr:', error);
      return null;
    }
  }

  // Regular event ID - encode as note
  try {
    return nip19.noteEncode(targetEventId);
  } catch (error) {
    console.error('[encodeTargetId] Failed to encode note:', error);
    return null;
  }
}
