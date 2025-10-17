import { signer, currentPubkey } from './auth';
import { get } from 'svelte/store';
import { makeBlossomAuthEvent, uploadBlob } from '@welshman/util';

/**
 * Default Blossom servers for file uploads
 * Users can add custom servers via NIP-XX (kind 10063) in the future
 */
const DEFAULT_BLOSSOM_SERVERS = [
  'https://blossom.primal.net',
  'https://cdn.satellite.earth',
  'https://nostr.download'
];

export interface UploadResult {
  url: string;
  sha256: string;
  size: number;
  type: string;
}

/**
 * Calculate SHA256 hash of file data
 */
async function calculateSHA256(data: ArrayBuffer): Promise<string> {
  const hashBuffer = await crypto.subtle.digest('SHA-256', data);
  const hashArray = Array.from(new Uint8Array(hashBuffer));
  return hashArray.map(b => b.toString(16).padStart(2, '0')).join('');
}

/**
 * Upload file utilities for Svelte 5
 * Uses Blossom protocol for file uploads
 */
export async function uploadFile(
  file: File,
  serverUrl?: string
): Promise<UploadResult> {
  const currentSigner = get(signer);
  const pubkey = get(currentPubkey);

  if (!currentSigner || !pubkey) {
    throw new Error('Must be logged in to upload files');
  }

  // Use provided server or default to first available
  const server = serverUrl || DEFAULT_BLOSSOM_SERVERS[0];

  try {
    // Calculate SHA256 hash of file
    const fileBuffer = await file.arrayBuffer();
    const hash = await calculateSHA256(fileBuffer);

    // Create Blossom auth event
    const authEventTemplate = makeBlossomAuthEvent({
      action: 'upload',
      server,
      hashes: [hash]
    });

    // Sign the auth event
    const authEvent = await currentSigner.sign(authEventTemplate);

    // Upload the blob
    const response = await uploadBlob(server, file, {
      authEvent
    });

    if (!response.ok) {
      const errorText = await response.text();
      throw new Error(`Upload failed: ${response.status} ${errorText}`);
    }

    // Parse response
    const result = await response.json();

    // Extract URL from response
    const url = result.url;
    if (!url) {
      throw new Error('Upload failed: no URL returned from server');
    }

    // Ensure file extension is present
    const fileExtension = file.name.split('.').pop() || file.type.split('/')[1];
    const finalUrl = url.includes('.') ? url : `${url}.${fileExtension}`;

    return {
      url: finalUrl,
      sha256: hash,
      size: file.size,
      type: file.type
    };
  } catch (error) {
    console.error('Blossom upload error:', error);
    throw error instanceof Error ? error : new Error('Unknown upload error');
  }
}

/**
 * Upload file with automatic retry across multiple servers
 */
export async function uploadFileWithRetry(file: File): Promise<UploadResult> {
  let lastError: Error | null = null;

  // Try each server in sequence until one succeeds
  for (const server of DEFAULT_BLOSSOM_SERVERS) {
    try {
      return await uploadFile(file, server);
    } catch (error) {
      console.warn(`Upload to ${server} failed:`, error);
      lastError = error instanceof Error ? error : new Error('Unknown error');
      // Continue to next server
    }
  }

  // All servers failed
  throw new Error(
    `Upload failed on all servers. Last error: ${lastError?.message || 'Unknown error'}`
  );
}

/**
 * Hook-style wrapper for upload functionality
 */
export function useUploadFile() {
  return {
    upload: uploadFile,
    uploadWithRetry: uploadFileWithRetry
  };
}
